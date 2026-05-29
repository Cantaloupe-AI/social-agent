//! `happycampr-carousels-cli` — a scriptable surface over the same backend the Tauri
//! app uses. Thin `clap` parser: all behaviour lives in
//! `happycampr_carousels_lib::cli`, which is unit-tested. This file only parses args,
//! formats the returned values, and sets the process exit code.

use happycampr_carousels_lib::cli::{
    self, ContentSource, ImportTarget,
};
use happycampr_carousels_lib::config;
use clap::{Args, Parser, Subcommand};
use std::path::PathBuf;
use std::thread;
use std::time::Duration;

#[derive(Parser)]
#[command(
    name = "happycampr-carousels-cli",
    about = "Create, populate, generate, and inspect happycampr carousels",
    long_about = "Scriptable access to the same SQLite/bun pipeline the \
                  Happycampr Carousels app drives. Useful for testing the core loop and \
                  for other agents."
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Carousel operations (create, list).
    Carousel {
        #[command(subcommand)]
        action: CarouselAction,
    },
    /// Slide operations (add).
    Slide {
        #[command(subcommand)]
        action: SlideAction,
    },
    /// Import a deck markdown file (split on `# Slide …` blocks) as slides.
    Import(ImportArgs),
    /// Kick off a generation. Blocks until done by default; opens the PDF.
    Generate(GenerateArgs),
    /// Print carousel + per-slide status and the tail of run.log.
    Status(StatusArgs),
    /// Open a carousel's generated PDF with the OS opener.
    Open(OpenArgs),
    /// Chart helpers: scaffold a plate-chart block, or a kitchen-sink deck.
    Chart {
        #[command(subcommand)]
        action: ChartAction,
    },
}

#[derive(Subcommand)]
enum ChartAction {
    /// Print one ready-to-edit plate-chart slide block to stdout.
    Scaffold {
        /// Chart family: column | bar | line | area | pie.
        #[arg(long)]
        family: String,
    },
    /// Build a deck with one plate-chart per Charts.css family.
    KitchenSink(KitchenSinkArgs),
}

#[derive(Args)]
struct KitchenSinkArgs {
    /// Import into this existing carousel.
    #[arg(long, conflicts_with = "new")]
    carousel: Option<String>,
    /// Create a new carousel for the deck.
    #[arg(long)]
    new: bool,
    /// Label for the new carousel (falls back to the deck title).
    #[arg(long)]
    label: Option<String>,
    /// Orientation for a new carousel (ignored with --carousel).
    #[arg(long)]
    orientation: Option<String>,
    /// Also write the deck markdown to this file.
    #[arg(long)]
    out: Option<PathBuf>,
}

#[derive(Subcommand)]
enum CarouselAction {
    /// Create an empty carousel.
    Create {
        #[arg(long)]
        label: String,
        /// vertical (1080×1350, default) or landscape (1920×1080).
        #[arg(long)]
        orientation: Option<String>,
    },
    /// List all carousels.
    List,
    /// Set the per-carousel agent model overrides (bun driver).
    SetModel(SetModelArgs),
}

#[derive(Args)]
struct SetModelArgs {
    /// Target carousel id.
    carousel_id: String,
    /// Set BOTH agents to this model id (shortcut for --impl + --manager).
    #[arg(long)]
    both: Option<String>,
    /// Implementation-agent model id (e.g. claude-sonnet-4-6).
    #[arg(long = "impl")]
    impl_model: Option<String>,
    /// Manager-agent model id.
    #[arg(long = "manager")]
    manager_model: Option<String>,
}

#[derive(Subcommand)]
enum SlideAction {
    /// Add one slide to an existing carousel.
    Add(SlideAddArgs),
}

#[derive(Args)]
struct SlideAddArgs {
    /// Target carousel id.
    carousel_id: String,
    /// Read slide content from this file.
    #[arg(long, group = "content_src")]
    file: Option<PathBuf>,
    /// Read slide content from stdin.
    #[arg(long, group = "content_src")]
    stdin: bool,
    /// Slide content as an inline string.
    #[arg(long, group = "content_src")]
    content: Option<String>,
    /// Optional explicit slide title.
    #[arg(long)]
    title: Option<String>,
}

#[derive(Args)]
struct ImportArgs {
    /// Append to this existing carousel.
    #[arg(long, conflicts_with = "new")]
    carousel: Option<String>,
    /// Create a new carousel for the imported slides.
    /// (`file` is already mandatory, so no `requires` needed.)
    #[arg(long)]
    new: bool,
    /// Label for the new carousel (falls back to the deck's front-matter title).
    #[arg(long)]
    label: Option<String>,
    /// Deck markdown file.
    #[arg(long)]
    file: PathBuf,
    /// Orientation for a new carousel (ignored with --carousel).
    #[arg(long)]
    orientation: Option<String>,
}

#[derive(Args)]
struct GenerateArgs {
    carousel_id: String,
    /// Don't auto-open the PDF when the run succeeds.
    #[arg(long)]
    no_open: bool,
    /// Re-render every slide, ignoring the markdown-unchanged skip. Use
    /// after editing the design spec, agent prompts, or brand assets so
    /// previously-accepted slides pick up the new rules.
    #[arg(long)]
    force: bool,
}

#[derive(Args)]
struct StatusArgs {
    carousel_id: String,
    /// How many trailing run.log lines to show (0 = the entire log).
    #[arg(long, default_value_t = 40)]
    log: usize,
    /// Poll every 2s until the run reaches a terminal state.
    #[arg(long)]
    watch: bool,
}

#[derive(Args)]
struct OpenArgs {
    carousel_id: String,
}

fn main() {
    if let Err(e) = run() {
        eprintln!("happycampr-carousels-cli: error: {e}");
        std::process::exit(1);
    }
}

fn base_dir() -> Result<PathBuf, String> {
    config::default_base_dir().map_err(|e| e.to_string())
}

fn run() -> Result<(), String> {
    let cli = Cli::parse();
    match cli.command {
        Command::Carousel { action } => match action {
            CarouselAction::Create { label, orientation } => {
                let conn = cli::open_db(&base_dir()?)?;
                let c = cli::cmd_carousel_create(&conn, &label, orientation.as_deref())?;
                println!(
                    "created carousel\n  id:          {}\n  label:       {}\n  slug:        {}\n  orientation: {}",
                    c.id,
                    c.label,
                    c.slug.as_deref().unwrap_or("-"),
                    c.orientation.as_str()
                );
            }
            CarouselAction::SetModel(a) => {
                let (im, mm) = if let Some(b) = a.both.as_deref() {
                    (Some(b), Some(b))
                } else {
                    (a.impl_model.as_deref(), a.manager_model.as_deref())
                };
                if im.is_none() && mm.is_none() {
                    return Err(
                        "provide --both <model>, or --impl <model> / --manager <model>"
                            .into(),
                    );
                }
                let conn = cli::open_db(&base_dir()?)?;
                let c = cli::cmd_carousel_set_model(&conn, &a.carousel_id, im, mm)?;
                println!(
                    "carousel {} models updated\n  impl:    {}\n  manager: {}",
                    c.id,
                    c.impl_model.as_deref().unwrap_or("(driver default)"),
                    c.manager_model.as_deref().unwrap_or("(driver default)")
                );
            }
            CarouselAction::List => {
                let conn = cli::open_db(&base_dir()?)?;
                let list = cli::cmd_carousel_list(&conn)?;
                if list.is_empty() {
                    println!("(no carousels)");
                }
                for c in list {
                    println!(
                        "{}  {:<10}  {:<24}  {}",
                        c.id,
                        c.status.as_str(),
                        c.slug.as_deref().unwrap_or("-"),
                        c.label
                    );
                }
            }
        },
        Command::Slide { action } => match action {
            SlideAction::Add(a) => {
                let src = if let Some(s) = a.content {
                    ContentSource::Inline(s)
                } else if let Some(f) = a.file {
                    ContentSource::File(f)
                } else if a.stdin {
                    ContentSource::Stdin
                } else {
                    return Err(
                        "provide slide content via --content, --file, or --stdin".into(),
                    );
                };
                let mut stdin = std::io::stdin();
                let content = cli::read_content(&src, &mut stdin)?;
                let conn = cli::open_db(&base_dir()?)?;
                let s = cli::cmd_slide_add(
                    &conn,
                    &a.carousel_id,
                    &content,
                    a.title.as_deref(),
                )?;
                println!(
                    "added slide {} (order {}) to carousel {}",
                    s.id, s.order_index, a.carousel_id
                );
            }
        },
        Command::Import(a) => {
            let deck_src = std::fs::read_to_string(&a.file)
                .map_err(|e| format!("reading {}: {e}", a.file.display()))?;
            let target = if let Some(id) = a.carousel {
                ImportTarget::Existing(id)
            } else if a.new {
                ImportTarget::New {
                    label: a.label,
                    orientation: cli::resolve_orientation(a.orientation.as_deref())?,
                }
            } else {
                return Err("specify --carousel <id> or --new".into());
            };
            let conn = cli::open_db(&base_dir()?)?;
            let r = cli::cmd_import(&conn, target, &deck_src)?;
            println!(
                "{} carousel {} (\"{}\") with {} slide(s)",
                if r.created { "created" } else { "appended to" },
                r.carousel_id,
                r.carousel_label,
                r.slide_count
            );
        }
        Command::Generate(a) => {
            let bd = base_dir()?;
            // Force re-render bypasses the driver's markdown-unchanged skip,
            // which can't see that the design spec / prompts / brand assets
            // changed. Propagated to the bun driver via the inherited env.
            if a.force {
                std::env::set_var("HAPPYCAMPR_FORCE", "1");
            }
            let outcome = cli::cmd_generate(
                &bd,
                &a.carousel_id,
                !a.no_open,
                cli::os_open,
            )?;
            let status = outcome.final_status.as_str();
            println!(
                "generation finished: {}\n  run dir: {}\n  pdf:     {}",
                status,
                outcome.run_dir.display(),
                outcome.pdf_path.as_deref().unwrap_or("(none)")
            );
            if outcome.final_status != happycampr_carousels_lib::types::CarouselStatus::Done {
                return Err(format!("run did not succeed (status: {status})"));
            }
        }
        Command::Status(a) => {
            let conn = cli::open_db(&base_dir()?)?;
            if a.watch {
                loop {
                    let r = cli::cmd_status(&conn, &a.carousel_id, a.log)?;
                    print_status(&r);
                    let terminal = matches!(
                        r.carousel.status,
                        happycampr_carousels_lib::types::CarouselStatus::Done
                            | happycampr_carousels_lib::types::CarouselStatus::Failed
                    );
                    if terminal {
                        break;
                    }
                    println!("--- (watching; ctrl-c to stop) ---");
                    thread::sleep(Duration::from_secs(2));
                }
            } else {
                let r = cli::cmd_status(&conn, &a.carousel_id, a.log)?;
                print_status(&r);
            }
        }
        Command::Open(a) => {
            let conn = cli::open_db(&base_dir()?)?;
            let path = cli::cmd_open(&conn, &a.carousel_id, cli::os_open)?;
            println!("opened {path}");
        }
        Command::Chart { action } => match action {
            ChartAction::Scaffold { family } => {
                println!("{}", cli::chart_slide_block(&family, 1, 1)?);
            }
            ChartAction::KitchenSink(a) => {
                let KitchenSinkArgs {
                    carousel,
                    new,
                    label,
                    orientation,
                    out,
                } = a;
                if let Some(ref p) = out {
                    std::fs::write(p, cli::kitchen_sink_deck())
                        .map_err(|e| format!("writing {}: {e}", p.display()))?;
                    println!("wrote deck → {}", p.display());
                }
                let target = if let Some(id) = carousel {
                    Some(ImportTarget::Existing(id))
                } else if new {
                    Some(ImportTarget::New {
                        label,
                        orientation: cli::resolve_orientation(orientation.as_deref())?,
                    })
                } else {
                    None
                };
                if let Some(t) = target {
                    let conn = cli::open_db(&base_dir()?)?;
                    let r = cli::cmd_chart_kitchen_sink(&conn, t)?;
                    println!(
                        "{} carousel {} (\"{}\") with {} chart slide(s)\n  next: happycampr-carousels-cli generate {}",
                        if r.created { "created" } else { "appended to" },
                        r.carousel_id,
                        r.carousel_label,
                        r.slide_count,
                        r.carousel_id
                    );
                } else if out.is_none() {
                    print!("{}", cli::kitchen_sink_deck());
                }
            }
        },
    }
    Ok(())
}

fn print_status(r: &cli::StatusReport) {
    let c = &r.carousel;
    println!("carousel {} (\"{}\")", c.id, c.label);
    println!("  status:      {}", c.status.as_str());
    println!("  slug:        {}", c.slug.as_deref().unwrap_or("-"));
    println!("  orientation: {}", c.orientation.as_str());
    println!("  pdf:         {}", c.pdf_path.as_deref().unwrap_or("(none)"));
    println!("  run dir:     {}", c.run_dir.as_deref().unwrap_or("(none)"));
    println!("  slides:");
    for s in &r.slides {
        let label = s
            .title
            .clone()
            .or_else(|| {
                s.content
                    .lines()
                    .find(|l| !l.trim().is_empty())
                    .map(|l| l.trim().to_string())
            })
            .unwrap_or_else(|| "(empty)".to_string());
        let label: String = label.chars().take(48).collect();
        print!("    [{}] {:<10} {}", s.order_index, s.status.as_str(), label);
        if let Some(e) = &s.last_error {
            print!("  !! {e}");
        }
        println!();
    }
    if !r.log_tail.is_empty() {
        println!("  --- run.log (last {} lines) ---", r.log_tail.lines().count());
        for line in r.log_tail.lines() {
            println!("  {line}");
        }
    }
}
