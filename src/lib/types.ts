export type ActivitySource = "git_commit" | "claude_code_session";

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
}
