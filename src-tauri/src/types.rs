use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ActivitySource {
    GitCommit,
    ClaudeCodeSession,
    Note,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Activity {
    pub id: String,
    pub source: ActivitySource,
    pub timestamp: DateTime<Utc>,
    pub summary: String,
    pub chip: String,
    pub included: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Entry {
    pub date: String,
    pub activities: Vec<Activity>,
    pub thoughts: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Config {
    #[serde(default)]
    pub git: GitConfig,
    pub claude_code: ClaudeCodeConfig,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub chrome_path: Option<PathBuf>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct GitConfig {
    #[serde(default)]
    pub repos: Vec<PathBuf>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ClaudeCodeConfig {
    pub projects_dir: PathBuf,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CarouselStatus {
    Idle,
    Generating,
    Done,
    Failed,
}

impl CarouselStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            CarouselStatus::Idle => "idle",
            CarouselStatus::Generating => "generating",
            CarouselStatus::Done => "done",
            CarouselStatus::Failed => "failed",
        }
    }

    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "idle" => Some(CarouselStatus::Idle),
            "generating" => Some(CarouselStatus::Generating),
            "done" => Some(CarouselStatus::Done),
            "failed" => Some(CarouselStatus::Failed),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Orientation {
    Vertical,
    Landscape,
}

impl Orientation {
    pub fn as_str(self) -> &'static str {
        match self {
            Orientation::Vertical => "vertical",
            Orientation::Landscape => "landscape",
        }
    }

    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "vertical" => Some(Orientation::Vertical),
            "landscape" => Some(Orientation::Landscape),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SlideStatus {
    Idle,
    Queued,
    Generating,
    Reviewing,
    Accepted,
    Failed,
}

impl SlideStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            SlideStatus::Idle => "idle",
            SlideStatus::Queued => "queued",
            SlideStatus::Generating => "generating",
            SlideStatus::Reviewing => "reviewing",
            SlideStatus::Accepted => "accepted",
            SlideStatus::Failed => "failed",
        }
    }

    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "idle" => Some(SlideStatus::Idle),
            "queued" => Some(SlideStatus::Queued),
            "generating" => Some(SlideStatus::Generating),
            "reviewing" => Some(SlideStatus::Reviewing),
            "accepted" => Some(SlideStatus::Accepted),
            "failed" => Some(SlideStatus::Failed),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Carousel {
    pub id: String,
    pub label: String,
    pub slug: Option<String>,
    pub status: CarouselStatus,
    pub pdf_path: Option<String>,
    pub run_dir: Option<String>,
    pub run_started_at: Option<DateTime<Utc>>,
    pub run_finished_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    /// Model id for the implementation agent. `None` means "use the
    /// driver's `DEFAULT_MODEL`" — we deliberately don't materialize
    /// the default string in the DB so changing it later is a one-line
    /// edit rather than a migration.
    pub impl_model: Option<String>,
    /// Model id for the manager (review) agent. Same `None` semantics
    /// as `impl_model`.
    pub manager_model: Option<String>,
    /// Per-carousel canvas orientation. Drives the renderer (puppeteer
    /// viewport + PDF page size) and the agents (which `landscape` or
    /// `vertical` branch of the design spec to follow). The DB column
    /// is NOT NULL with DEFAULT 'vertical', so older rows and
    /// no-orientation inserts naturally read back as Vertical.
    pub orientation: Orientation,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Slide {
    pub id: String,
    pub carousel_id: String,
    pub order_index: i64,
    pub content: String,
    /// User-edited slide title. `None` means "auto-derive from content"
    /// (the frontend uses the first `# header` line or the first
    /// non-empty line); `Some(s)` is whatever the user explicitly typed
    /// in the title input.
    pub title: Option<String>,
    pub status: SlideStatus,
    pub last_error: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SlideVersion {
    pub id: String,
    pub slide_id: String,
    pub version_number: i64,
    pub html_path: String,
    pub screenshot_path: Option<String>,
    pub pdf_path: Option<String>,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SlideFeedback {
    pub id: String,
    pub slide_version_id: String,
    pub accepted: bool,
    pub feedback: String,
    pub created_at: DateTime<Utc>,
}
