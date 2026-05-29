//! Tauri-free carousel-generation core.
//!
//! Everything `commands::generate_carousel_pdf` does *except* parking the
//! child in the in-memory `GenerationProcesses` map (a UI-cancel concern)
//! lives here, so the Tauri command and the `happycampr-carousels-cli` binary drive a
//! generation through one code path. Per CLAUDE.md §conventions/errors:
//! run-dir naming and the crash-finalize safety net exist in exactly one
//! place — `next_run_dir` and `finalize` below.
//!
//! Paths are injected (`base_dir`, `root`) rather than read from globals so
//! the pipeline is testable against a temp dir, mirroring the
//! `db::open(base_dir)` / `config::load_config_from(base_dir)` precedent.

use crate::types::CarouselStatus;
use std::fs::OpenOptions;
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::thread;

fn open_db(base_dir: &Path) -> Result<rusqlite::Connection, String> {
    crate::db::open(base_dir).map_err(|e| e.to_string())
}

/// Resolve the repo root (parent of the `src-tauri` Cargo manifest dir).
///
/// `CARGO_MANIFEST_DIR` is `/…/social-agent/src-tauri` at build time for
/// every binary in this crate (the Tauri app and `happycampr-carousels-cli` alike).
pub fn repo_root() -> Result<PathBuf, String> {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    manifest_dir
        .parent()
        .map(|p| p.to_path_buf())
        .ok_or_else(|| "could not resolve repo root from CARGO_MANIFEST_DIR".to_string())
}

/// Path to the bun driver script, given a repo root.
pub fn driver_script(root: &Path) -> PathBuf {
    root.join("scripts").join("generate-carousel.ts")
}

/// Resolve the `bun` executable to an absolute path when possible.
///
/// macOS GUI-launched apps (Finder/Dock/`open`) inherit a minimal PATH
/// (`/usr/bin:/bin:/usr/sbin:/sbin`) that excludes `~/.bun/bin`, so a bare
/// `Command::new("bun")` fails at spawn with ENOENT. Observed: carousel
/// runs 1–4 died ~3 ms in with `spawn failed: No such file or directory
/// (os error 2)`, while the same carousel succeeded to spawn from
/// `happycampr-carousels-cli` (a shell child that *does* inherit `~/.bun/bin`).
///
/// Strategy, in order: an explicit `HAPPYCAMPR_BUN` override, the known
/// install locations, then a bare `"bun"` fallback so terminal/CLI
/// launches keep working via the inherited PATH. `spawn_driver` also
/// prepends the resolved dir to the child's PATH so `bun` finds anything
/// it shells out to even under the stripped GUI environment.
fn resolve_bun() -> PathBuf {
    if let Some(p) = std::env::var_os("HAPPYCAMPR_BUN") {
        let p = PathBuf::from(p);
        if p.is_file() {
            return p;
        }
    }
    let mut candidates: Vec<PathBuf> = Vec::new();
    if let Some(home) = std::env::var_os("HOME") {
        candidates.push(PathBuf::from(&home).join(".bun/bin/bun"));
    }
    candidates.push(PathBuf::from("/opt/homebrew/bin/bun"));
    candidates.push(PathBuf::from("/usr/local/bin/bun"));
    candidates
        .into_iter()
        .find(|c| c.is_file())
        // Last resort: bare name. Resolves via the inherited PATH, which
        // works for the `happycampr-carousels-cli` (shell child) launch path.
        .unwrap_or_else(|| PathBuf::from("bun"))
}

/// Pick the next `run-N` directory under `<base_dir>/carousels/<slug>/`.
///
/// We scan the existing entries, take the max numeric suffix, add 1, and
/// `create_dir` (atomic — fails if it already exists). This is the only
/// place run-dir naming happens; the bun script just trusts the path it's
/// given.
pub fn next_run_dir(base_dir: &Path, slug: &str) -> Result<PathBuf, String> {
    let carousel_dir = crate::config::carousels_dir(base_dir).join(slug);
    std::fs::create_dir_all(&carousel_dir)
        .map_err(|e| format!("creating {}: {e}", carousel_dir.display()))?;

    let mut max = 0u32;
    for entry in std::fs::read_dir(&carousel_dir)
        .map_err(|e| format!("reading {}: {e}", carousel_dir.display()))?
    {
        let entry = entry.map_err(|e| e.to_string())?;
        let name = entry.file_name();
        let Some(s) = name.to_str() else { continue };
        if let Some(rest) = s.strip_prefix("run-") {
            if let Ok(n) = rest.parse::<u32>() {
                if n > max {
                    max = n;
                }
            }
        }
    }
    let next = max + 1;
    let dir = carousel_dir.join(format!("run-{next}"));
    std::fs::create_dir(&dir).map_err(|e| {
        format!(
            "creating run dir {}: {e} (race with another driver?)",
            dir.display()
        )
    })?;
    Ok(dir)
}

/// Tee a child pipe into both `run.log` and the parent's stderr,
/// line-by-line, prefixing each line so the source is unambiguous.
///
/// Returning early on a read error is fine — the child has either exited
/// or its pipe has gone away; the watcher will pick that up.
pub fn spawn_pipe_tee(
    label: &'static str,
    pipe: impl std::io::Read + Send + 'static,
    log_path: PathBuf,
) {
    thread::spawn(move || {
        let reader = BufReader::new(pipe);
        for line_res in reader.lines() {
            let line = match line_res {
                Ok(l) => l,
                Err(e) => {
                    eprintln!("happycampr: pipe-tee[{label}] read error: {e}");
                    return;
                }
            };
            let stamped = format!(
                "[{}] [{}] {}\n",
                chrono::Utc::now().to_rfc3339(),
                label,
                line
            );
            // Always echo to parent stderr so `tauri dev` / the CLI shows it inline.
            eprint!("{stamped}");
            // And append to the run.log file for the UI to read.
            match OpenOptions::new().create(true).append(true).open(&log_path) {
                Ok(mut f) => {
                    if let Err(e) = f.write_all(stamped.as_bytes()) {
                        eprintln!(
                            "happycampr: pipe-tee[{label}] log write failed ({}): {e}",
                            log_path.display()
                        );
                    }
                }
                Err(e) => {
                    eprintln!(
                        "happycampr: pipe-tee[{label}] log open failed ({}): {e}",
                        log_path.display()
                    );
                }
            }
        }
    });
}

/// Append a freshly-stamped line to `run.log`. Used to record `[exit]`
/// lines that didn't come through a pipe.
pub fn append_log_line(log_path: &Path, line: &str) {
    let stamped = format!("[{}] {}\n", chrono::Utc::now().to_rfc3339(), line);
    eprint!("{stamped}");
    match OpenOptions::new().create(true).append(true).open(log_path) {
        Ok(mut f) => {
            if let Err(e) = f.write_all(stamped.as_bytes()) {
                eprintln!(
                    "happycampr: append_log_line failed ({}): {e}",
                    log_path.display()
                );
            }
        }
        Err(e) => {
            eprintln!(
                "happycampr: append_log_line open failed ({}): {e}",
                log_path.display()
            );
        }
    }
}

/// Everything `prepare_run` needs to hand back to the supervisor.
#[derive(Debug, Clone)]
pub struct RunCtx {
    pub run_dir: PathBuf,
    pub log_path: PathBuf,
    pub slug: String,
    pub label: String,
    pub slide_count: usize,
}

/// Validate inputs, pick the run dir, and persist run-start state.
///
/// Mirrors the pre-spawn half of the old `generate_carousel_pdf`: the
/// driver script and carousel are checked *before* any DB write so a
/// validation failure never leaves the carousel stuck on 'generating'.
pub fn prepare_run(
    base_dir: &Path,
    root: &Path,
    carousel_id: &str,
) -> Result<RunCtx, String> {
    let script = driver_script(root);
    if !script.exists() {
        return Err(format!("driver script not found at {}", script.display()));
    }

    let (slug, label, slide_count) = {
        let conn = open_db(base_dir)?;
        let carousel = crate::db::get_carousel(&conn, carousel_id)
            .map_err(|e| e.to_string())?
            .ok_or_else(|| format!("carousel {carousel_id} not found"))?;
        let slug = carousel.slug.clone().ok_or_else(|| {
            format!("carousel {carousel_id} has no slug — reopen the app once")
        })?;
        let slides =
            crate::db::list_slides(&conn, carousel_id).map_err(|e| e.to_string())?;
        if slides.is_empty() {
            return Err("Carousel has no slides".into());
        }
        (slug, carousel.label, slides.len())
    };

    let run_dir = next_run_dir(base_dir, &slug)?;
    let log_path = run_dir.join("run.log");
    append_log_line(
        &log_path,
        &format!(
            "[supervisor] starting bun driver for carousel \"{label}\" ({slide_count} slide(s))"
        ),
    );

    // Persist run start so the UI can find run.log immediately, even if bun
    // crashes before its first DB write. Also reset every slide's status to
    // 'queued' so the progress UI doesn't show stale ✓/✗ from a prior run.
    {
        let conn = open_db(base_dir)?;
        crate::db::set_carousel_run_started(
            &conn,
            carousel_id,
            run_dir.to_string_lossy().as_ref(),
        )
        .map_err(|e| e.to_string())?;
        crate::db::reset_slides_for_run(&conn, carousel_id)
            .map_err(|e| e.to_string())?;
    }

    Ok(RunCtx {
        run_dir,
        log_path,
        slug,
        label,
        slide_count,
    })
}

/// Best-effort: flip the carousel to `Failed` and record `why` to
/// `run.log`. Every `spawn_driver` failure path funnels through here so
/// the cleanup lives in one place. Per CLAUDE.md §conventions/errors: a
/// cleanup failure (db locked, disk full, …) is surfaced, never swallowed.
fn mark_run_failed(base_dir: &Path, carousel_id: &str, log_path: &Path, why: &str) {
    match open_db(base_dir) {
        Ok(conn) => {
            if let Err(cleanup_err) = crate::db::set_carousel_run_finished(
                &conn,
                carousel_id,
                CarouselStatus::Failed,
                None,
            ) {
                let msg = format!(
                    "[supervisor] {why} AND failed to mark carousel failed \
                     during cleanup: {cleanup_err}"
                );
                eprintln!("{msg}");
                append_log_line(log_path, &msg);
            }
        }
        Err(open_err) => {
            let msg = format!(
                "[supervisor] {why} AND could not open db to mark carousel \
                 failed: {open_err}"
            );
            eprintln!("{msg}");
            append_log_line(log_path, &msg);
        }
    }
    append_log_line(log_path, &format!("[supervisor] {why}"));
}

/// Spawn the bun driver and wire stdout/stderr into `run.log`.
///
/// On any failure the carousel is marked `Failed` so the UI doesn't sit on
/// 'generating'. The driver-script existence check is repeated here (it's
/// also done in `prepare_run`) so this `pub fn` is safe to call directly
/// without bypassing it.
pub fn spawn_driver(
    base_dir: &Path,
    root: &Path,
    carousel_id: &str,
    run_dir: &Path,
    log_path: &Path,
) -> Result<Child, String> {
    let script = driver_script(root);
    if !script.exists() {
        let why = format!("driver script not found at {}", script.display());
        mark_run_failed(base_dir, carousel_id, log_path, &why);
        return Err(why);
    }
    let bun = resolve_bun();
    let mut cmd = Command::new(&bun);
    cmd.arg("run")
        .arg(&script)
        .arg(carousel_id)
        .arg(run_dir)
        .current_dir(root)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    // Prepend the resolved bun dir to the child's PATH so the GUI launch's
    // stripped PATH (`/usr/bin:/bin:…`) doesn't break anything bun itself
    // shells out to. Skipped when `resolve_bun` fell back to the bare name
    // (parent is empty) — there the inherited PATH is already correct.
    if let Some(bun_dir) = bun.parent().filter(|d| !d.as_os_str().is_empty()) {
        let existing = std::env::var_os("PATH").unwrap_or_default();
        let mut joined = std::ffi::OsString::from(bun_dir);
        if !existing.is_empty() {
            joined.push(":");
            joined.push(&existing);
        }
        cmd.env("PATH", joined);
    }
    let mut child = cmd.spawn().map_err(|e| {
        mark_run_failed(
            base_dir,
            carousel_id,
            log_path,
            &format!("spawn failed: {e}"),
        );
        format!("spawning bun driver: {e}")
    })?;

    if let Some(stdout) = child.stdout.take() {
        spawn_pipe_tee("bun-stdout", stdout, log_path.to_path_buf());
    }
    if let Some(stderr) = child.stderr.take() {
        spawn_pipe_tee("bun-stderr", stderr, log_path.to_path_buf());
    }
    Ok(child)
}

/// Single crash-finalize routine, reachable from every supervisor exit
/// path (Tauri watcher thread *and* the CLI's blocking wait).
///
/// Idempotent: if the carousel already reached a terminal state (the bun
/// driver called `set_carousel_run_finished` itself), this only logs. If
/// it's still 'generating', the driver died mid-run — flip to 'failed'
/// loudly so the UI doesn't sit on a stuck status.
///
/// The Tauri-only bits (clearing the `Child` handle, removing the entry
/// from `GenerationProcesses`) stay in `commands::finalize_run`, which
/// calls this for the DB half.
///
/// `base_dir` is `Option` so the `[exit]` line is written **first and
/// unconditionally** even on the (macOS-impossible) path where the caller
/// couldn't resolve the data dir — the original supervisor invariant was
/// "log-first, always"; passing `None` keeps that while honestly noting
/// the DB safety net was skipped (no silent failure).
pub fn finalize(
    base_dir: Option<&Path>,
    carousel_id: &str,
    log_path: &Path,
    reason: &str,
) {
    // Log-first: written before any fallible step, on every exit path.
    append_log_line(log_path, &format!("[supervisor] [exit] {reason}"));

    let Some(base_dir) = base_dir else {
        let msg = "[supervisor] no data dir — DB crash-finalize skipped; \
                   carousel status may be stuck (re-running generate self-heals)";
        eprintln!("{msg}");
        append_log_line(log_path, msg);
        return;
    };

    let conn = match open_db(base_dir) {
        Ok(c) => c,
        Err(e) => {
            let msg = format!(
                "[supervisor] open_db during finalize failed: {e} \
                 — carousel status may be stuck"
            );
            eprintln!("{msg}");
            append_log_line(log_path, &msg);
            return;
        }
    };
    let status = match crate::db::get_carousel(&conn, carousel_id) {
        Ok(Some(c)) => c.status,
        Ok(None) => {
            let msg = "[supervisor] carousel disappeared from db";
            eprintln!("{msg}");
            append_log_line(log_path, msg);
            return;
        }
        Err(e) => {
            let msg = format!("[supervisor] get_carousel during finalize: {e}");
            eprintln!("{msg}");
            append_log_line(log_path, &msg);
            return;
        }
    };
    if status == CarouselStatus::Generating {
        match crate::db::set_carousel_run_finished(
            &conn,
            carousel_id,
            CarouselStatus::Failed,
            None,
        ) {
            Ok(()) => append_log_line(
                log_path,
                "[supervisor] marked carousel failed (status was still 'generating')",
            ),
            Err(e) => {
                let msg = format!(
                    "[supervisor] FAILED to mark carousel failed: {e} \
                     — UI will show 'generating' until manually fixed"
                );
                eprintln!("{msg}");
                append_log_line(log_path, &msg);
            }
        }
    } else {
        append_log_line(
            log_path,
            &format!("[supervisor] carousel already in terminal state '{status:?}'"),
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db;
    use crate::types::{CarouselStatus, SlideStatus};
    use std::sync::atomic::{AtomicU32, Ordering};

    static COUNTER: AtomicU32 = AtomicU32::new(0);

    /// Fresh temp base dir, same shape as the config-module test helper
    /// (no `tempfile` crate — declined in STATUS.md).
    fn fresh_base_dir(tag: &str) -> PathBuf {
        let n = COUNTER.fetch_add(1, Ordering::SeqCst);
        let dir = std::env::temp_dir().join(format!(
            "happycampr-carousels-test-gen-{}-{}-{}",
            tag,
            std::process::id(),
            n
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    /// A `root` whose `scripts/generate-carousel.ts` exists, so the
    /// `prepare_run` script check passes without depending on the real repo.
    fn fake_root_with_driver(base_dir: &Path) -> PathBuf {
        let root = base_dir.join("repo");
        std::fs::create_dir_all(root.join("scripts")).unwrap();
        std::fs::write(root.join("scripts").join("generate-carousel.ts"), "// stub").unwrap();
        root
    }

    fn seed_carousel_with_slide(base_dir: &Path) -> String {
        let conn = db::open(base_dir).unwrap();
        let c = db::create_carousel(&conn, "Gen Test").unwrap();
        let s = db::create_slide(&conn, &c.id).unwrap();
        db::update_slide_content(&conn, &s.id, "# Slide 1 :: cover\nhi").unwrap();
        c.id
    }

    #[test]
    fn prepare_run_errors_when_carousel_missing() {
        let base = fresh_base_dir("missing");
        let root = fake_root_with_driver(&base);
        let err = prepare_run(&base, &root, "no-such-id").unwrap_err();
        assert!(err.contains("not found"), "got: {err}");
    }

    #[test]
    fn prepare_run_errors_when_no_slides() {
        let base = fresh_base_dir("noslides");
        let root = fake_root_with_driver(&base);
        let id = {
            let conn = db::open(&base).unwrap();
            db::create_carousel(&conn, "Empty").unwrap().id
        };
        let err = prepare_run(&base, &root, &id).unwrap_err();
        assert!(err.contains("no slides"), "got: {err}");
    }

    #[test]
    fn prepare_run_errors_when_driver_script_missing() {
        let base = fresh_base_dir("noscript");
        let id = seed_carousel_with_slide(&base);
        // `root` with no scripts/generate-carousel.ts.
        let bogus_root = base.join("not-a-repo");
        std::fs::create_dir_all(&bogus_root).unwrap();
        let err = prepare_run(&base, &bogus_root, &id).unwrap_err();
        assert!(err.contains("driver script not found"), "got: {err}");
    }

    #[test]
    fn prepare_run_happy_path_flips_state_and_increments_run_dir() {
        let base = fresh_base_dir("happy");
        let root = fake_root_with_driver(&base);
        let id = seed_carousel_with_slide(&base);

        let ctx = prepare_run(&base, &root, &id).expect("prepare_run");
        assert!(ctx.run_dir.ends_with("run-1"));
        assert_eq!(ctx.slide_count, 1);
        assert!(ctx.log_path.exists(), "opening log line should create run.log");

        let conn = db::open(&base).unwrap();
        let c = db::get_carousel(&conn, &id).unwrap().unwrap();
        assert_eq!(c.status, CarouselStatus::Generating);
        assert_eq!(c.run_dir.as_deref(), Some(ctx.run_dir.to_string_lossy().as_ref()));
        for s in db::list_slides(&conn, &id).unwrap() {
            assert_eq!(s.status, SlideStatus::Queued);
        }
        drop(conn);

        let ctx2 = prepare_run(&base, &root, &id).expect("second prepare_run");
        assert!(ctx2.run_dir.ends_with("run-2"), "got: {:?}", ctx2.run_dir);
    }

    #[test]
    fn spawn_driver_missing_script_marks_failed() {
        let base = fresh_base_dir("spawnfail");
        let root = fake_root_with_driver(&base);
        let id = seed_carousel_with_slide(&base);
        let ctx = prepare_run(&base, &root, &id).expect("prepare_run");

        // Deterministic, no global-state mutation: a root with no
        // scripts/generate-carousel.ts trips spawn_driver's own
        // script-exists guard, which funnels through `mark_run_failed`
        // (the same cleanup the OS-spawn-error branch uses).
        let missing_root = base.join("does-not-exist");
        let result =
            spawn_driver(&base, &missing_root, &id, &ctx.run_dir, &ctx.log_path);

        assert!(result.is_err(), "missing driver script must error");
        assert!(
            result.unwrap_err().contains("driver script not found"),
            "expected the clear guard message"
        );
        let conn = db::open(&base).unwrap();
        let c = db::get_carousel(&conn, &id).unwrap().unwrap();
        assert_eq!(c.status, CarouselStatus::Failed);
    }

    #[test]
    fn finalize_flips_stuck_generating_to_failed() {
        let base = fresh_base_dir("finalize-stuck");
        let id = seed_carousel_with_slide(&base);
        let conn = db::open(&base).unwrap();
        db::set_carousel_status(&conn, &id, CarouselStatus::Generating).unwrap();
        drop(conn);

        let log_path = base.join("run.log");
        finalize(Some(&base), &id, &log_path, "bun driver exited with status: 1");

        let conn = db::open(&base).unwrap();
        let c = db::get_carousel(&conn, &id).unwrap().unwrap();
        assert_eq!(c.status, CarouselStatus::Failed);
    }

    #[test]
    fn finalize_leaves_terminal_status_untouched() {
        let base = fresh_base_dir("finalize-done");
        let id = seed_carousel_with_slide(&base);
        let conn = db::open(&base).unwrap();
        db::set_carousel_run_finished(&conn, &id, CarouselStatus::Done, Some("/x.pdf"))
            .unwrap();
        drop(conn);

        let log_path = base.join("run.log");
        finalize(Some(&base), &id, &log_path, "bun driver exited with status: 0");

        let conn = db::open(&base).unwrap();
        let c = db::get_carousel(&conn, &id).unwrap().unwrap();
        assert_eq!(c.status, CarouselStatus::Done);
        assert_eq!(c.pdf_path.as_deref(), Some("/x.pdf"));
    }

    #[test]
    fn finalize_missing_carousel_does_not_panic() {
        let base = fresh_base_dir("finalize-missing");
        // db must exist (migrated) so open succeeds; carousel absent.
        let _ = db::open(&base).unwrap();
        let log_path = base.join("run.log");
        finalize(Some(&base), "ghost-id", &log_path, "exited");
        // No panic == pass; log line recorded.
        assert!(log_path.exists());
    }

    #[test]
    fn finalize_none_base_dir_writes_exit_line_and_skips_db() {
        // The macOS-impossible "no data dir" path: the [exit] line MUST
        // still be written (log-first invariant) and we must not panic.
        let base = fresh_base_dir("finalize-none");
        let log_path = base.join("run.log");
        finalize(None, "irrelevant", &log_path, "bun driver exited with status: 0");
        let log = std::fs::read_to_string(&log_path).unwrap();
        assert!(log.contains("[supervisor] [exit] bun driver exited"));
        assert!(log.contains("DB crash-finalize skipped"));
    }
}
