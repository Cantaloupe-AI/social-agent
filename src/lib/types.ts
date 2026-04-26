export type ActivitySource = "git_commit" | "claude_code_session" | "note";

export interface Activity {
  id: string;
  source: ActivitySource;
  timestamp: string;
  summary: string;
  chip: string;
  included: boolean;
}

export interface Entry {
  date: string;
  activities: Activity[];
  thoughts: string;
  created_at: string;
  updated_at: string;
}

export interface GitConfig {
  repos: string[];
}

export interface ClaudeCodeConfig {
  projects_dir: string;
}

export interface Config {
  git: GitConfig;
  claude_code: ClaudeCodeConfig;
  chrome_path?: string;
}

export type CarouselStatus = "idle" | "generating" | "done" | "failed";

export type SlideStatus =
  | "idle"
  | "queued"
  | "generating"
  | "reviewing"
  | "accepted"
  | "failed";

export interface Carousel {
  id: string;
  label: string;
  slug: string | null;
  status: CarouselStatus;
  pdf_path: string | null;
  run_dir: string | null;
  run_started_at: string | null;
  run_finished_at: string | null;
  created_at: string;
  updated_at: string;
  /**
   * Per-carousel model overrides for the bun driver's two agents.
   * `null` means "use the driver's DEFAULT_MODEL" — the literal default
   * string is intentionally not materialized in the DB so the default
   * can change in one place (`scripts/generate-carousel.ts`).
   */
  impl_model: string | null;
  manager_model: string | null;
}

export interface Slide {
  id: string;
  carousel_id: string;
  order_index: number;
  content: string;
  /**
   * User-edited title. `null` means "auto-derive" — the UI computes a
   * default from the first `# header` or the first non-empty line of
   * `content`. A non-null string is whatever the user explicitly typed.
   */
  title: string | null;
  status: SlideStatus;
  last_error: string | null;
  created_at: string;
  updated_at: string;
}

export interface SlideVersion {
  id: string;
  slide_id: string;
  version_number: number;
  html_path: string;
  screenshot_path: string | null;
  pdf_path: string | null;
  created_at: string;
}

export interface SlideFeedback {
  id: string;
  slide_version_id: string;
  accepted: boolean;
  feedback: string;
  created_at: string;
}
