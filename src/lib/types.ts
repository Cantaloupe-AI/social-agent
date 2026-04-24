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
}

export interface Slide {
  id: string;
  carousel_id: string;
  order_index: number;
  content: string;
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
