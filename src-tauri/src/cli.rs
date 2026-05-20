//! `cantalog-cli` command handlers.
//!
//! The binary (`src/bin/cantalog-cli.rs`) is a thin `clap` parser; all
//! behaviour lives here as pure functions returning `Result<T, String>`
//! values (a struct, not printed output) so it is unit-testable against a
//! temp-dir DB without spawning a process. Side-effecting inputs (stdin,
//! the PDF opener) are injected, per CLAUDE.md §test_philosophy ("no mocks
//! where a parameter will do").

use crate::generation;
use crate::types::{Carousel, Orientation, Slide};
use rusqlite::Connection;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::thread;
use std::time::Duration;

/// Open the file-backed DB under `base_dir` (same target as the Tauri app).
pub fn open_db(base_dir: &Path) -> Result<Connection, String> {
    crate::db::open(base_dir).map_err(|e| e.to_string())
}

pub fn resolve_orientation(raw: Option<&str>) -> Result<Orientation, String> {
    match raw {
        None => Ok(Orientation::Vertical),
        Some(s) => Orientation::from_str(s)
            .ok_or_else(|| format!("invalid orientation '{s}' (expected vertical|landscape)")),
    }
}

// ---------------------------------------------------------------------------
// Deck parsing
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct FrontMatter {
    pub title: Option<String>,
    pub slug: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Deck {
    pub front_matter: Option<FrontMatter>,
    /// One entry per `# Slide …` block, heading line included, trimmed.
    pub slides: Vec<String>,
}

/// Parse a deck markdown file into per-slide content blocks.
///
/// Format (see `design/carousel_example.md`): an optional leading
/// `---`-delimited doc front-matter block, then one or more blocks each
/// starting with `# Slide ` at column 0. The front-matter is documentary —
/// it is **not** injected into slide content; only `title`/`slug` are read
/// (to seed `--new` when no `--label` is given).
///
/// Fenced code blocks are tracked (``` ``` ``` and `~~~`, matched by the
/// opening marker per CommonMark) so a `# Slide …` line inside a fence does
/// NOT start a new slide. An **unclosed** fence is an error, not silently
/// swallowed. Any non-front-matter content **before the first `# Slide`**
/// is intentionally dropped as documentary preamble — put real content
/// inside a `# Slide …` block.
pub fn parse_deck(src: &str) -> Result<Deck, String> {
    let normalized = src.replace("\r\n", "\n");
    let lines: Vec<&str> = normalized.split('\n').collect();

    let mut idx = 0;
    // Skip leading blank lines before optional front matter.
    while idx < lines.len() && lines[idx].trim().is_empty() {
        idx += 1;
    }

    let mut front_matter = None;
    if idx < lines.len() && lines[idx].trim() == "---" {
        let mut end = None;
        for (i, line) in lines.iter().enumerate().skip(idx + 1) {
            if line.trim() == "---" {
                end = Some(i);
                break;
            }
        }
        let close = end.ok_or_else(|| {
            "unterminated front matter (missing closing '---')".to_string()
        })?;
        let mut fm = FrontMatter::default();
        for line in &lines[idx + 1..close] {
            if let Some((k, v)) = line.split_once(':') {
                match k.trim().to_ascii_lowercase().as_str() {
                    "title" => fm.title = non_empty(v),
                    "slug" => fm.slug = non_empty(v),
                    _ => {}
                }
            }
        }
        front_matter = Some(fm);
        idx = close + 1;
    }

    let mut slides: Vec<String> = Vec::new();
    let mut current: Option<Vec<&str>> = None;
    // CommonMark: a fence is closed only by the same marker char it opened
    // with. Track the opener so `~~~` ↔ ``` ``` ``` don't cross-toggle.
    let mut fence: Option<char> = None;
    for line in &lines[idx..] {
        let ts = line.trim_start();
        match fence {
            None => {
                if ts.starts_with("```") {
                    fence = Some('`');
                } else if ts.starts_with("~~~") {
                    fence = Some('~');
                }
            }
            Some('`') if ts.starts_with("```") => fence = None,
            Some('~') if ts.starts_with("~~~") => fence = None,
            _ => {}
        }
        let is_boundary = fence.is_none() && line.starts_with("# Slide ");
        if is_boundary {
            if let Some(block) = current.take() {
                slides.push(block.join("\n").trim().to_string());
            }
            current = Some(vec![*line]);
        } else if let Some(block) = current.as_mut() {
            block.push(line);
        }
        // Lines before the first boundary (after front matter) are
        // intentionally dropped — documentary preamble (see doc-comment).
    }
    if let Some(block) = current.take() {
        slides.push(block.join("\n").trim().to_string());
    }

    if fence.is_some() {
        return Err(
            "unclosed fenced code block in deck markdown — a `# Slide` \
             heading may have been swallowed; close the fence"
                .to_string(),
        );
    }
    if slides.is_empty() {
        return Err("no '# Slide' blocks found in deck markdown".to_string());
    }
    Ok(Deck {
        front_matter,
        slides,
    })
}

fn non_empty(s: &str) -> Option<String> {
    let t = s.trim();
    if t.is_empty() {
        None
    } else {
        Some(t.to_string())
    }
}

// ---------------------------------------------------------------------------
// Slide content source (file / inline / stdin) — reader injected for tests
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ContentSource {
    File(PathBuf),
    Inline(String),
    Stdin,
}

pub fn read_content<R: Read>(
    src: &ContentSource,
    stdin: &mut R,
) -> Result<String, String> {
    match src {
        ContentSource::Inline(s) => Ok(s.clone()),
        ContentSource::File(p) => std::fs::read_to_string(p)
            .map_err(|e| format!("reading {}: {e}", p.display())),
        ContentSource::Stdin => {
            let mut buf = String::new();
            stdin
                .read_to_string(&mut buf)
                .map_err(|e| format!("reading stdin: {e}"))?;
            Ok(buf)
        }
    }
}

// ---------------------------------------------------------------------------
// Carousel / slide authoring
// ---------------------------------------------------------------------------

pub fn cmd_carousel_create(
    conn: &Connection,
    label: &str,
    orientation: Option<&str>,
) -> Result<Carousel, String> {
    let orientation = resolve_orientation(orientation)?;
    crate::db::create_carousel_with_orientation(conn, label, orientation)
        .map_err(|e| e.to_string())
}

pub fn cmd_carousel_list(conn: &Connection) -> Result<Vec<Carousel>, String> {
    crate::db::list_carousels(conn).map_err(|e| e.to_string())
}

/// Set the per-carousel agent model overrides used by the bun driver.
/// Unspecified args preserve the current value (so `--impl` alone does not
/// wipe `manager_model` back to the default). Pass an empty string to clear
/// one back to the driver default.
pub fn cmd_carousel_set_model(
    conn: &Connection,
    carousel_id: &str,
    impl_model: Option<&str>,
    manager_model: Option<&str>,
) -> Result<Carousel, String> {
    let current = crate::db::get_carousel(conn, carousel_id)
        .map_err(|e| e.to_string())?
        .ok_or_else(|| format!("carousel {carousel_id} not found"))?;
    let impl_eff = impl_model
        .map(str::to_string)
        .or_else(|| current.impl_model.clone());
    let manager_eff = manager_model
        .map(str::to_string)
        .or_else(|| current.manager_model.clone());
    crate::db::update_carousel_models(
        conn,
        carousel_id,
        impl_eff.as_deref(),
        manager_eff.as_deref(),
    )
    .map_err(|e| e.to_string())?;
    crate::db::get_carousel(conn, carousel_id)
        .map_err(|e| e.to_string())?
        .ok_or_else(|| "carousel disappeared after update".to_string())
}

pub fn cmd_slide_add(
    conn: &Connection,
    carousel_id: &str,
    content: &str,
    title: Option<&str>,
) -> Result<Slide, String> {
    if crate::db::get_carousel(conn, carousel_id)
        .map_err(|e| e.to_string())?
        .is_none()
    {
        return Err(format!("carousel {carousel_id} not found"));
    }
    let slide = crate::db::create_slide(conn, carousel_id).map_err(|e| e.to_string())?;
    crate::db::update_slide_content(conn, &slide.id, content).map_err(|e| e.to_string())?;
    if let Some(t) = title {
        crate::db::update_slide_title(conn, &slide.id, Some(t)).map_err(|e| e.to_string())?;
    }
    crate::db::get_slide(conn, &slide.id)
        .map_err(|e| e.to_string())?
        .ok_or_else(|| "slide disappeared after insert".to_string())
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ImportTarget {
    Existing(String),
    New {
        label: Option<String>,
        orientation: Orientation,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImportResult {
    pub carousel_id: String,
    pub carousel_label: String,
    pub slide_count: usize,
    pub created: bool,
}

pub fn cmd_import(
    conn: &Connection,
    target: ImportTarget,
    deck_src: &str,
) -> Result<ImportResult, String> {
    let deck = parse_deck(deck_src)?;

    let (carousel, created) = match target {
        ImportTarget::Existing(id) => {
            let c = crate::db::get_carousel(conn, &id)
                .map_err(|e| e.to_string())?
                .ok_or_else(|| format!("carousel {id} not found"))?;
            (c, false)
        }
        ImportTarget::New { label, orientation } => {
            let label = label
                .or_else(|| {
                    deck.front_matter
                        .as_ref()
                        .and_then(|fm| fm.title.clone())
                })
                .ok_or_else(|| {
                    "--new requires --label or a `title:` in the deck front-matter"
                        .to_string()
                })?;
            let c = crate::db::create_carousel_with_orientation(conn, &label, orientation)
                .map_err(|e| e.to_string())?;
            (c, true)
        }
    };

    for block in &deck.slides {
        let slide = crate::db::create_slide(conn, &carousel.id).map_err(|e| e.to_string())?;
        crate::db::update_slide_content(conn, &slide.id, block).map_err(|e| e.to_string())?;
    }

    Ok(ImportResult {
        carousel_id: carousel.id,
        carousel_label: carousel.label,
        slide_count: deck.slides.len(),
        created,
    })
}

// ---------------------------------------------------------------------------
// Chart authoring (plate-chart) + kitchen-sink deck
// ---------------------------------------------------------------------------

/// The Charts.css families `plate-chart` supports — mirror of
/// `design/carousel.manifest.json#chart.families`. Data that fits none of
/// these must fall back to `plate-list` per the design contracts; the CLI
/// only scaffolds the supported set.
pub const CHART_FAMILIES: [&str; 5] = ["column", "bar", "line", "area", "pie"];

/// Roman numeral for folio chrome (`N / TOTAL`, see manifest §4). General
/// enough for any deck size; deck sizes are 3..=10 in practice.
fn roman(mut n: usize) -> String {
    const TABLE: [(usize, &str); 13] = [
        (1000, "M"), (900, "CM"), (500, "D"), (400, "CD"), (100, "C"),
        (90, "XC"), (50, "L"), (40, "XL"), (10, "X"), (9, "IX"),
        (5, "V"), (4, "IV"), (1, "I"),
    ];
    let mut s = String::new();
    for (v, sym) in TABLE {
        while n >= v {
            s.push_str(sym);
            n -= v;
        }
    }
    s
}

/// The `chart:`-block body for each family (everything after the heading +
/// `folio:` line). Authoring schema mirrors
/// `design/carousel_example.md` → "Reference — plate-chart". Series are
/// capped at two because happycampr is a two-chromatic palette
/// (graham + moss); see `design/ambiguous.md` §13.
fn family_body(family: &str) -> &'static str {
    match family {
        "column" => {
            "headline: |\n  Signups keep\n  {{accent:climbing}}.\nchart:\n  \
             family: column\n  axis:\n    x: Quarter\n    y: Signups\n    \
             max: 1200\n  series:\n    - name: 2025\n      color: primary\n      \
             data: { Q1: 420, Q2: 610, Q3: 780, Q4: 1120 }\n    - name: 2026\n      \
             color: secondary\n      data: { Q1: 540, Q2: 700, Q3: 910, Q4: 1180 }\n\
             caption: |\n  Two years, same four quarters.\n"
        }
        "bar" => {
            "headline: |\n  Where signups\n  {{accent:come from}}.\nchart:\n  \
             family: bar\n  axis:\n    x: Channel\n    y: Signups\n    max: 900\n  \
             series:\n    - name: Signups\n      color: primary\n      \
             data: { Referral: 860, Search: 540, Social: 410, Direct: 230, Email: 120 }\n\
             caption: |\n  Ranked, highest first.\n"
        }
        "line" => {
            "headline: |\n  Retention is\n  {{accent:holding}}.\nchart:\n  \
             family: line\n  axis:\n    x: Week\n    y: Active %\n    max: 100\n  \
             series:\n    - name: Cohort A\n      color: primary\n      \
             data: { W1: 100, W2: 82, W3: 71, W4: 65, W5: 62 }\n    - name: Cohort B\n      \
             color: secondary\n      data: { W1: 100, W2: 88, W3: 80, W4: 76, W5: 74 }\n\
             caption: |\n  Percent still active by week.\n"
        }
        "area" => {
            "headline: |\n  Revenue,\n  {{accent:cumulative}}.\nchart:\n  \
             family: area\n  axis:\n    x: Month\n    y: $K\n    max: 480\n  \
             series:\n    - name: 2026\n      color: primary\n      \
             data: { Jan: 40, Feb: 95, Mar: 165, Apr: 250, May: 350, Jun: 470 }\n\
             caption: |\n  Running total through June.\n"
        }
        "pie" => {
            "headline: |\n  Traffic\n  {{accent:mix}}.\nchart:\n  family: pie\n  \
             series:\n    - name: Share\n      color: primary\n      \
             data: { Referral: 46, Search: 28, Social: 18, Direct: 8 }\n\
             caption: |\n  Share of sessions, last 30 days.\n"
        }
        _ => unreachable!("family_body called with unvalidated family"),
    }
}

/// Build one ready-to-edit `# Slide N :: plate-chart` block for `family`.
/// `index`/`total` drive the `folio:` chrome.
pub fn chart_slide_block(
    family: &str,
    index: usize,
    total: usize,
) -> Result<String, String> {
    if !CHART_FAMILIES.contains(&family) {
        return Err(format!(
            "unknown chart family '{family}' (expected one of: {})",
            CHART_FAMILIES.join(", ")
        ));
    }
    let folio = format!("{} / {}", roman(index), roman(total));
    let body = family_body(family);
    Ok(format!(
        "# Slide {index} :: plate-chart\n\nfolio: {folio}\n{body}"
    ))
}

/// A complete deck exercising every Charts.css family, one per slide.
/// Round-trips through [`parse_deck`]; the leading HTML-comment note is
/// documentary preamble (dropped on import, kept in any `--out` file).
pub fn kitchen_sink_deck() -> String {
    let total = CHART_FAMILIES.len();
    let mut out = String::from(
        "---\ntitle: Charts Kitchen Sink\nslug: charts-kitchen-sink\n---\n\n\
         <!-- One plate-chart per Charts.css family. happycampr is a two-\n\
         chromatic palette (graham + moss), so each demo stays <= 2 series.\n\
         Edit the data freely, then: cantalog-cli generate <carousel_id>. -->\n",
    );
    for (i, fam) in CHART_FAMILIES.iter().enumerate() {
        out.push('\n');
        out.push_str(
            &chart_slide_block(fam, i + 1, total).expect("built-in family is valid"),
        );
        out.push('\n');
    }
    out
}

/// Build the kitchen-sink deck and import it via [`cmd_import`].
pub fn cmd_chart_kitchen_sink(
    conn: &Connection,
    target: ImportTarget,
) -> Result<ImportResult, String> {
    cmd_import(conn, target, &kitchen_sink_deck())
}

// ---------------------------------------------------------------------------
// Status
// ---------------------------------------------------------------------------

/// Last `max_lines` lines of the carousel's `run.log`, or "" if no run has
/// started / the file is missing. Mirrors `commands::read_run_log_tail`.
pub fn tail_run_log(carousel: &Carousel, max_lines: usize) -> Result<String, String> {
    let Some(run_dir) = carousel.run_dir.as_ref() else {
        return Ok(String::new());
    };
    let log_path = PathBuf::from(run_dir).join("run.log");
    if !log_path.exists() {
        return Ok(String::new());
    }
    let contents = std::fs::read_to_string(&log_path)
        .map_err(|e| format!("reading {}: {e}", log_path.display()))?;
    if max_lines == 0 {
        return Ok(contents);
    }
    let lines: Vec<&str> = contents.lines().collect();
    let start = lines.len().saturating_sub(max_lines);
    Ok(lines[start..].join("\n"))
}

#[derive(Debug, Clone)]
pub struct StatusReport {
    pub carousel: Carousel,
    pub slides: Vec<Slide>,
    pub log_tail: String,
}

pub fn cmd_status(
    conn: &Connection,
    carousel_id: &str,
    log_lines: usize,
) -> Result<StatusReport, String> {
    let carousel = crate::db::get_carousel(conn, carousel_id)
        .map_err(|e| e.to_string())?
        .ok_or_else(|| format!("carousel {carousel_id} not found"))?;
    let slides = crate::db::list_slides(conn, carousel_id).map_err(|e| e.to_string())?;
    let log_tail = tail_run_log(&carousel, log_lines)?;
    Ok(StatusReport {
        carousel,
        slides,
        log_tail,
    })
}

// ---------------------------------------------------------------------------
// Open
// ---------------------------------------------------------------------------

/// Default opener: hand the path to macOS `open`. Injected as a parameter
/// so tests assert the resolved path without spawning a process.
pub fn os_open(p: &Path) -> Result<(), String> {
    std::process::Command::new("open")
        .arg(p)
        .spawn()
        .map(|_| ())
        .map_err(|e| format!("opening {}: {e}", p.display()))
}

pub fn cmd_open<F: FnOnce(&Path) -> Result<(), String>>(
    conn: &Connection,
    carousel_id: &str,
    opener: F,
) -> Result<String, String> {
    let carousel = crate::db::get_carousel(conn, carousel_id)
        .map_err(|e| e.to_string())?
        .ok_or_else(|| format!("carousel {carousel_id} not found"))?;
    let pdf_path = carousel.pdf_path.ok_or_else(|| {
        format!(
            "carousel '{}' has no PDF yet (status: {})",
            carousel.label,
            carousel.status.as_str()
        )
    })?;
    if !Path::new(&pdf_path).exists() {
        return Err(format!("PDF not found on disk: {pdf_path}"));
    }
    opener(Path::new(&pdf_path))?;
    Ok(pdf_path)
}

// ---------------------------------------------------------------------------
// Generate (orchestration; the real agent loop is exercised via manual E2E,
// its pieces are unit-tested in `generation`)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub struct GenerateOutcome {
    pub run_dir: PathBuf,
    pub final_status: crate::types::CarouselStatus,
    pub pdf_path: Option<String>,
}

/// Prepare + spawn the bun driver, block until the child exits, run the
/// shared `generation::finalize` safety net, then — if `open_pdf` and the
/// run succeeded — hand the PDF to `opener`.
///
/// Always blocking by design: bun's stdout/stderr are tee'd to this
/// process's stderr by `generation::spawn_driver`, so progress streams to
/// the terminal live and the run is supervised end-to-end. (A `--detach`
/// mode was considered and removed — it left run.log capture dead and the
/// run unsupervised; an async caller can just background this command.)
pub fn cmd_generate<F: FnOnce(&Path) -> Result<(), String>>(
    base_dir: &Path,
    carousel_id: &str,
    open_pdf: bool,
    opener: F,
) -> Result<GenerateOutcome, String> {
    let root = generation::repo_root()?;
    let ctx = generation::prepare_run(base_dir, &root, carousel_id)?;
    let mut child =
        generation::spawn_driver(base_dir, &root, carousel_id, &ctx.run_dir, &ctx.log_path)?;

    let reason: String = loop {
        thread::sleep(Duration::from_millis(150));
        match child.try_wait() {
            Ok(Some(s)) => break format!("bun driver exited with {s}"),
            Ok(None) => continue,
            Err(e) => break format!("try_wait error: {e}"),
        }
    };
    generation::finalize(Some(base_dir), carousel_id, &ctx.log_path, &reason);

    let conn = open_db(base_dir)?;
    let carousel = crate::db::get_carousel(&conn, carousel_id)
        .map_err(|e| e.to_string())?
        .ok_or_else(|| format!("carousel {carousel_id} disappeared after run"))?;

    if open_pdf
        && carousel.status == crate::types::CarouselStatus::Done
    {
        if let Some(p) = carousel.pdf_path.as_ref() {
            opener(Path::new(p))?;
        }
    }

    Ok(GenerateOutcome {
        run_dir: ctx.run_dir,
        final_status: carousel.status,
        pdf_path: carousel.pdf_path,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db;
    use crate::types::{CarouselStatus, SlideStatus};
    use std::io::Cursor;
    use std::sync::atomic::{AtomicU32, Ordering};

    static COUNTER: AtomicU32 = AtomicU32::new(0);

    fn fresh_base_dir(tag: &str) -> PathBuf {
        let n = COUNTER.fetch_add(1, Ordering::SeqCst);
        let dir = std::env::temp_dir().join(format!(
            "cantalog-test-cli-{}-{}-{}",
            tag,
            std::process::id(),
            n
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn example_deck() -> String {
        let p = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/deck/example.md");
        std::fs::read_to_string(p).expect("read example deck fixture")
    }

    // ---- parse_deck -----------------------------------------------------

    #[test]
    fn parse_deck_splits_five_blocks_with_front_matter() {
        let deck = parse_deck(&example_deck()).expect("parse");
        assert_eq!(deck.slides.len(), 5, "expected 5 slide blocks");
        let fm = deck.front_matter.expect("front matter");
        assert_eq!(fm.title.as_deref(), Some("Manifests for Agents"));
        assert_eq!(fm.slug.as_deref(), Some("manifests-for-agents"));

        // Order + heading retained, front-matter NOT injected.
        assert!(deck.slides[0].starts_with("# Slide 1 :: cover"));
        assert!(deck.slides[4].starts_with("# Slide 5 :: takeaway"));
        assert!(!deck.slides[0].contains("plate_count"));
        assert!(!deck.slides[0].contains("# Slide 2"));
    }

    #[test]
    fn parse_deck_does_not_split_on_hash_inside_fenced_code() {
        let deck = parse_deck(&example_deck()).expect("parse");
        // The plate-code block keeps its fenced `#` line and is a single block.
        let code_block = &deck.slides[2];
        assert!(code_block.starts_with("# Slide 3 :: plate-code"));
        assert!(code_block.contains("# this hash is inside a fenced block"));
        assert!(!code_block.contains("# Slide 4 :: plate-list"));
    }

    #[test]
    fn parse_deck_fenced_slide_heading_is_not_a_boundary() {
        let src = "# Slide 1 :: a\nbody\n\n```\n# Slide 99 :: trap\n```\n\n# Slide 2 :: b\nmore\n";
        let deck = parse_deck(src).expect("parse");
        assert_eq!(deck.slides.len(), 2, "fenced heading must not split");
        assert!(deck.slides[0].contains("# Slide 99 :: trap"));
    }

    #[test]
    fn parse_deck_unclosed_fence_errors() {
        // H2: an unclosed fence must NOT silently swallow later slides.
        let src = "# Slide 1 :: a\nbody\n```\n# Slide 2 :: b\nstuff\n";
        let err = parse_deck(src).unwrap_err();
        assert!(err.contains("unclosed fenced code block"), "got: {err}");
    }

    #[test]
    fn parse_deck_mismatched_fence_markers_do_not_cross_toggle() {
        // M1: a ``` line must not close a ~~~ fence. Slide 2 stays inside
        // the ~~~ fence; only Slide 3 (after the matching ~~~) is a boundary.
        let src = "# Slide 1 :: a\n~~~\ncode\n```\n# Slide 2 :: b\n~~~\n# Slide 3 :: c\nbody\n";
        let deck = parse_deck(src).expect("parse");
        assert_eq!(deck.slides.len(), 2, "expected 2 blocks, got {}", deck.slides.len());
        assert!(deck.slides[0].starts_with("# Slide 1 :: a"));
        assert!(deck.slides[0].contains("# Slide 2 :: b"), "Slide 2 stays fenced");
        assert!(deck.slides[1].starts_with("# Slide 3 :: c"));
    }

    #[test]
    fn parse_deck_without_front_matter() {
        let src = "# Slide 1 :: a\nhello\n\n# Slide 2 :: b\nworld\n";
        let deck = parse_deck(src).expect("parse");
        assert!(deck.front_matter.is_none());
        assert_eq!(deck.slides.len(), 2);
    }

    #[test]
    fn parse_deck_handles_crlf() {
        let src = example_deck().replace('\n', "\r\n");
        let deck = parse_deck(&src).expect("parse crlf");
        assert_eq!(deck.slides.len(), 5);
        assert!(!deck.slides[0].contains('\r'));
    }

    #[test]
    fn parse_deck_unterminated_front_matter_errors() {
        let src = "---\ntitle: x\n\n# Slide 1 :: a\nbody\n";
        let err = parse_deck(src).unwrap_err();
        assert!(err.contains("unterminated front matter"), "got: {err}");
    }

    #[test]
    fn parse_deck_no_slides_errors() {
        let src = "---\ntitle: x\n---\n\njust prose, no slide blocks\n";
        let err = parse_deck(src).unwrap_err();
        assert!(err.contains("no '# Slide'"), "got: {err}");
    }

    // ---- read_content ---------------------------------------------------

    #[test]
    fn read_content_inline_file_and_stdin() {
        let mut empty = Cursor::new(Vec::new());
        assert_eq!(
            read_content(&ContentSource::Inline("hi".into()), &mut empty).unwrap(),
            "hi"
        );

        let base = fresh_base_dir("readcontent");
        let f = base.join("s.md");
        std::fs::write(&f, "from file").unwrap();
        assert_eq!(
            read_content(&ContentSource::File(f), &mut empty).unwrap(),
            "from file"
        );

        let mut stdin = Cursor::new(b"from stdin".to_vec());
        assert_eq!(
            read_content(&ContentSource::Stdin, &mut stdin).unwrap(),
            "from stdin"
        );
    }

    // ---- carousel / slide ----------------------------------------------

    #[test]
    fn carousel_create_and_list() {
        let base = fresh_base_dir("ccreate");
        let conn = open_db(&base).unwrap();
        let c = cmd_carousel_create(&conn, "Launch Week", None).unwrap();
        assert_eq!(c.slug.as_deref(), Some("launch-week"));
        assert_eq!(c.orientation, Orientation::Vertical);
        let list = cmd_carousel_list(&conn).unwrap();
        assert!(list.iter().any(|x| x.id == c.id));
    }

    #[test]
    fn carousel_create_rejects_bad_orientation() {
        let base = fresh_base_dir("cbadorient");
        let conn = open_db(&base).unwrap();
        let err = cmd_carousel_create(&conn, "X", Some("sideways")).unwrap_err();
        assert!(err.contains("invalid orientation"), "got: {err}");
    }

    #[test]
    fn carousel_set_model_preserves_unspecified_and_errors_on_missing() {
        let base = fresh_base_dir("setmodel");
        let conn = open_db(&base).unwrap();
        let c = cmd_carousel_create(&conn, "Models", None).unwrap();
        assert_eq!(c.impl_model, None);
        assert_eq!(c.manager_model, None);

        // --impl only must NOT wipe manager_model.
        let c1 =
            cmd_carousel_set_model(&conn, &c.id, Some("claude-sonnet-4-6"), None).unwrap();
        assert_eq!(c1.impl_model.as_deref(), Some("claude-sonnet-4-6"));
        assert_eq!(c1.manager_model, None);

        // Setting both (the `--both` path) sticks.
        let c2 = cmd_carousel_set_model(
            &conn,
            &c.id,
            Some("claude-sonnet-4-6"),
            Some("claude-sonnet-4-6"),
        )
        .unwrap();
        assert_eq!(c2.impl_model.as_deref(), Some("claude-sonnet-4-6"));
        assert_eq!(c2.manager_model.as_deref(), Some("claude-sonnet-4-6"));

        // Whitespace clears back to default (NULL); impl preserved.
        let c3 = cmd_carousel_set_model(&conn, &c.id, None, Some("  ")).unwrap();
        assert_eq!(c3.impl_model.as_deref(), Some("claude-sonnet-4-6"));
        assert_eq!(c3.manager_model, None);

        let err = cmd_carousel_set_model(&conn, "ghost", Some("x"), None).unwrap_err();
        assert!(err.contains("not found"), "got: {err}");
    }

    #[test]
    fn slide_add_with_and_without_title_and_missing_carousel() {
        let base = fresh_base_dir("sadd");
        let conn = open_db(&base).unwrap();
        let c = cmd_carousel_create(&conn, "Deck", None).unwrap();

        let s1 = cmd_slide_add(&conn, &c.id, "# Slide 1 :: cover\nhi", None).unwrap();
        assert_eq!(s1.content, "# Slide 1 :: cover\nhi");
        assert_eq!(s1.title, None);

        let s2 = cmd_slide_add(&conn, &c.id, "body", Some("My Title")).unwrap();
        assert_eq!(s2.title.as_deref(), Some("My Title"));
        assert!(s2.order_index > s1.order_index);

        let err = cmd_slide_add(&conn, "nope", "x", None).unwrap_err();
        assert!(err.contains("not found"), "got: {err}");
    }

    // ---- import ---------------------------------------------------------

    #[test]
    fn import_new_with_explicit_label_creates_ordered_slides() {
        let base = fresh_base_dir("impnew");
        let conn = open_db(&base).unwrap();
        let res = cmd_import(
            &conn,
            ImportTarget::New {
                label: Some("My Carousel".into()),
                orientation: Orientation::Landscape,
            },
            &example_deck(),
        )
        .unwrap();
        assert!(res.created);
        assert_eq!(res.slide_count, 5);
        assert_eq!(res.carousel_label, "My Carousel");

        let c = db::get_carousel(&conn, &res.carousel_id).unwrap().unwrap();
        assert_eq!(c.orientation, Orientation::Landscape);
        let slides = db::list_slides(&conn, &res.carousel_id).unwrap();
        assert_eq!(slides.len(), 5);
        assert!(slides[0].content.starts_with("# Slide 1 :: cover"));
        assert!(slides[4].content.starts_with("# Slide 5 :: takeaway"));
    }

    #[test]
    fn import_new_falls_back_to_front_matter_title() {
        let base = fresh_base_dir("impfm");
        let conn = open_db(&base).unwrap();
        let res = cmd_import(
            &conn,
            ImportTarget::New {
                label: None,
                orientation: Orientation::Vertical,
            },
            &example_deck(),
        )
        .unwrap();
        assert_eq!(res.carousel_label, "Manifests for Agents");
    }

    #[test]
    fn import_new_without_label_or_title_errors() {
        let base = fresh_base_dir("impnolabel");
        let conn = open_db(&base).unwrap();
        let src = "# Slide 1 :: a\nbody\n";
        let err = cmd_import(
            &conn,
            ImportTarget::New {
                label: None,
                orientation: Orientation::Vertical,
            },
            src,
        )
        .unwrap_err();
        assert!(err.contains("--new requires"), "got: {err}");
    }

    #[test]
    fn import_into_existing_appends_and_missing_errors() {
        let base = fresh_base_dir("impexist");
        let conn = open_db(&base).unwrap();
        let c = cmd_carousel_create(&conn, "Target", None).unwrap();
        cmd_slide_add(&conn, &c.id, "pre-existing", None).unwrap();

        let res = cmd_import(
            &conn,
            ImportTarget::Existing(c.id.clone()),
            &example_deck(),
        )
        .unwrap();
        assert!(!res.created);
        assert_eq!(res.slide_count, 5);
        assert_eq!(db::list_slides(&conn, &c.id).unwrap().len(), 6);

        let err = cmd_import(
            &conn,
            ImportTarget::Existing("ghost".into()),
            &example_deck(),
        )
        .unwrap_err();
        assert!(err.contains("not found"), "got: {err}");
    }

    // ---- chart ----------------------------------------------------------

    #[test]
    fn chart_slide_block_rejects_unknown_family() {
        let err = chart_slide_block("scatter", 1, 1).unwrap_err();
        assert!(err.contains("unknown chart family 'scatter'"), "got: {err}");
        assert!(err.contains("column, bar, line, area, pie"), "got: {err}");
    }

    #[test]
    fn chart_slide_block_has_heading_folio_and_family() {
        let b = chart_slide_block("line", 3, 7).unwrap();
        assert!(b.starts_with("# Slide 3 :: plate-chart"), "got: {b}");
        assert!(b.contains("folio: III / VII"), "got: {b}");
        assert!(b.contains("family: line"), "got: {b}");
        assert!(b.contains("headline: |"));
        assert!(b.contains("caption: |"));
    }

    #[test]
    fn kitchen_sink_deck_round_trips_one_slide_per_family() {
        let deck = parse_deck(&kitchen_sink_deck()).expect("parse kitchen sink");
        assert_eq!(deck.slides.len(), CHART_FAMILIES.len());
        let fm = deck.front_matter.expect("front matter");
        assert_eq!(fm.title.as_deref(), Some("Charts Kitchen Sink"));
        assert_eq!(fm.slug.as_deref(), Some("charts-kitchen-sink"));
        for (i, fam) in CHART_FAMILIES.iter().enumerate() {
            let block = &deck.slides[i];
            assert!(
                block.starts_with(&format!("# Slide {} :: plate-chart", i + 1)),
                "slide {i} heading: {block}"
            );
            assert!(block.contains(&format!("family: {fam}")), "slide {i} family");
        }
        // The documentary HTML-comment preamble is dropped on parse.
        assert!(!deck.slides[0].contains("<!--"));
    }

    #[test]
    fn cmd_chart_kitchen_sink_creates_a_carousel_of_charts() {
        let base = fresh_base_dir("kitchensink");
        let conn = open_db(&base).unwrap();
        let res = cmd_chart_kitchen_sink(
            &conn,
            ImportTarget::New {
                label: None,
                orientation: Orientation::Vertical,
            },
        )
        .unwrap();
        assert!(res.created);
        assert_eq!(res.carousel_label, "Charts Kitchen Sink");
        assert_eq!(res.slide_count, CHART_FAMILIES.len());
        let slides = db::list_slides(&conn, &res.carousel_id).unwrap();
        assert_eq!(slides.len(), CHART_FAMILIES.len());
        assert!(slides[0].content.contains("family: column"));
        assert!(slides[4].content.contains("family: pie"));
    }

    // ---- status ---------------------------------------------------------

    #[test]
    fn status_reports_carousel_slides_and_log_tail() {
        let base = fresh_base_dir("status");
        let conn = open_db(&base).unwrap();
        let c = cmd_carousel_create(&conn, "S", None).unwrap();
        cmd_slide_add(&conn, &c.id, "a", None).unwrap();

        // No run yet → empty tail.
        let r0 = cmd_status(&conn, &c.id, 10).unwrap();
        assert_eq!(r0.carousel.status, CarouselStatus::Idle);
        assert_eq!(r0.slides.len(), 1);
        assert_eq!(r0.slides[0].status, SlideStatus::Idle);
        assert_eq!(r0.log_tail, "");

        // Seed a run dir + run.log and point the carousel at it.
        let run_dir = base.join("run-1");
        std::fs::create_dir_all(&run_dir).unwrap();
        let body: String =
            (1..=20).map(|i| format!("line {i}\n")).collect();
        std::fs::write(run_dir.join("run.log"), body).unwrap();
        db::set_carousel_run_started(&conn, &c.id, run_dir.to_string_lossy().as_ref())
            .unwrap();

        let r1 = cmd_status(&conn, &c.id, 5).unwrap();
        assert_eq!(r1.carousel.status, CarouselStatus::Generating);
        let tail_lines: Vec<&str> = r1.log_tail.lines().collect();
        assert_eq!(tail_lines.len(), 5);
        assert_eq!(tail_lines[4], "line 20");

        let err = cmd_status(&conn, "ghost", 5).unwrap_err();
        assert!(err.contains("not found"), "got: {err}");
    }

    // ---- open -----------------------------------------------------------

    #[test]
    fn open_errors_when_no_pdf_and_succeeds_with_injected_opener() {
        let base = fresh_base_dir("open");
        let conn = open_db(&base).unwrap();
        let c = cmd_carousel_create(&conn, "O", None).unwrap();

        let err = cmd_open(&conn, &c.id, |_| Ok(())).unwrap_err();
        assert!(err.contains("no PDF yet"), "got: {err}");

        // Point at a real file and assert the opener receives it.
        let pdf = base.join("out.pdf");
        std::fs::write(&pdf, b"%PDF-1.4").unwrap();
        db::set_carousel_run_finished(
            &conn,
            &c.id,
            CarouselStatus::Done,
            Some(pdf.to_string_lossy().as_ref()),
        )
        .unwrap();

        let seen = std::cell::RefCell::new(None);
        let returned = cmd_open(&conn, &c.id, |p| {
            *seen.borrow_mut() = Some(p.to_path_buf());
            Ok(())
        })
        .unwrap();
        assert_eq!(returned, pdf.to_string_lossy());
        assert_eq!(seen.into_inner(), Some(pdf));

        // Missing-on-disk path errors before the opener runs.
        db::set_carousel_run_finished(
            &conn,
            &c.id,
            CarouselStatus::Done,
            Some("/no/such/file.pdf"),
        )
        .unwrap();
        let err = cmd_open(&conn, &c.id, |_| Ok(())).unwrap_err();
        assert!(err.contains("PDF not found on disk"), "got: {err}");
    }
}
