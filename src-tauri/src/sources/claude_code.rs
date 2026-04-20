use crate::types::{Activity, ActivitySource};
use anyhow::Result;
use chrono::{DateTime, Local, Utc};
use std::path::Path;
use std::time::SystemTime;

const SUMMARY_CHAR_LIMIT: usize = 80;

/// Returns the first human-typed user message summary from a Claude Code `.jsonl` file.
///
/// Walks lines, skipping malformed ones and control records. A "line" yields a summary when
/// its top-level `type == "user"`, `message.role == "user"`, and `message.content` is either
/// a string or an array that contains a `{type:"text", text:"..."}` block at the top level.
/// Tool-result wrapper user lines (no top-level text block) are skipped because they surface
/// as bookkeeping, not human input.
pub fn first_user_message(jsonl: &str) -> Option<String> {
    for line in jsonl.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let Ok(v) = serde_json::from_str::<serde_json::Value>(line) else {
            continue;
        };
        if v.get("type").and_then(|t| t.as_str()) != Some("user") {
            continue;
        }
        let Some(msg) = v.get("message") else {
            continue;
        };
        if msg.get("role").and_then(|r| r.as_str()) != Some("user") {
            continue;
        }
        let Some(content) = msg.get("content") else {
            continue;
        };
        if let Some(s) = content.as_str() {
            return Some(truncate_summary(s, SUMMARY_CHAR_LIMIT));
        }
        if let Some(arr) = content.as_array() {
            for block in arr {
                if block.get("type").and_then(|t| t.as_str()) == Some("text") {
                    if let Some(text) = block.get("text").and_then(|t| t.as_str()) {
                        return Some(truncate_summary(text, SUMMARY_CHAR_LIMIT));
                    }
                }
            }
        }
    }
    None
}

/// Truncates `s` to at most `max_chars` characters (by char, not byte), flattening embedded
/// newlines to spaces so the summary sits cleanly in a single row. Appends `…` if anything
/// was cut so the user knows there's more context in the session.
pub fn truncate_summary(s: &str, max_chars: usize) -> String {
    let flat: String = s
        .chars()
        .map(|c| if c == '\n' || c == '\r' || c == '\t' { ' ' } else { c })
        .collect();
    let flat = flat.trim();
    let taken: String = flat.chars().take(max_chars).collect();
    if flat.chars().count() > max_chars {
        format!("{taken}…")
    } else {
        taken
    }
}

// TODO(spec): Claude Code encodes project paths by replacing `/` with `-`, which is lossy
// (hidden dirs become `--`, and real dashes in directory names are indistinguishable from
// separators). We follow the spec literally: swap `-` back to `/`. Displays like
// `/Users/josh/code/rss-fetcher` as `/Users/josh/code/rss/fetcher` — unfortunate but matches
// the spec. Also strips a single leading `/` so the chip reads as a short project label
// rather than an absolute path.
pub fn unescape_project_name(dir_name: &str) -> String {
    let decoded = dir_name.replace('-', "/");
    decoded.strip_prefix('/').unwrap_or(&decoded).to_string()
}

/// Checks whether `mtime` falls on the same local-timezone calendar day as `reference`.
/// The reference date is injected so tests don't have to freeze `Local::now()`.
pub fn is_same_local_day(mtime: SystemTime, reference: DateTime<Local>) -> bool {
    let mtime_local: DateTime<Local> = DateTime::<Local>::from(mtime);
    mtime_local.date_naive() == reference.date_naive()
}

/// Scans `projects_dir/<project>/<session>.jsonl` files modified today (per `reference`)
/// and returns one Activity per eligible session. Missing or unreadable directories become
/// empty results, not errors, so one bad project doesn't break the rest.
pub fn scan_projects(
    projects_dir: &Path,
    reference: DateTime<Local>,
) -> Result<Vec<Activity>> {
    let Ok(entries) = std::fs::read_dir(projects_dir) else {
        eprintln!(
            "cantalog: claude projects dir not readable: {}",
            projects_dir.display()
        );
        return Ok(Vec::new());
    };

    let mut activities = Vec::new();
    for entry in entries.flatten() {
        if !entry.file_type().map(|t| t.is_dir()).unwrap_or(false) {
            continue;
        }
        let project_dir = entry.path();
        let project_chip =
            unescape_project_name(&entry.file_name().to_string_lossy());

        let Ok(children) = std::fs::read_dir(&project_dir) else {
            continue;
        };
        for child in children.flatten() {
            let path = child.path();
            if path.extension().and_then(|e| e.to_str()) != Some("jsonl") {
                continue;
            }
            let Ok(meta) = child.metadata() else { continue };
            let Ok(mtime) = meta.modified() else { continue };
            if !is_same_local_day(mtime, reference) {
                continue;
            }
            let Ok(contents) = std::fs::read_to_string(&path) else {
                continue;
            };
            let Some(summary) = first_user_message(&contents) else {
                continue;
            };
            let id = path
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or("unknown")
                .to_string();
            activities.push(Activity {
                id: format!("claude:{id}"),
                source: ActivitySource::ClaudeCodeSession,
                timestamp: DateTime::<Utc>::from(mtime),
                summary,
                chip: project_chip.clone(),
                included: true,
            });
        }
    }
    Ok(activities)
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;
    use std::fs::OpenOptions;
    use std::sync::atomic::{AtomicU32, Ordering};
    use std::time::Duration;

    static COUNTER: AtomicU32 = AtomicU32::new(0);

    const TYPED_PROMPT: &str =
        include_str!("../../tests/fixtures/claude/typed_prompt.jsonl");
    const ARRAY_CONTENT: &str =
        include_str!("../../tests/fixtures/claude/array_content.jsonl");
    const TOOL_RESULT_THEN_USER: &str =
        include_str!("../../tests/fixtures/claude/tool_result_then_user.jsonl");
    const MALFORMED: &str =
        include_str!("../../tests/fixtures/claude/malformed.jsonl");
    const NO_USER: &str =
        include_str!("../../tests/fixtures/claude/no_user.jsonl");
    const LONG_PROMPT: &str =
        include_str!("../../tests/fixtures/claude/long_prompt.jsonl");

    fn fresh_temp(tag: &str) -> std::path::PathBuf {
        let n = COUNTER.fetch_add(1, Ordering::SeqCst);
        let p = std::env::temp_dir().join(format!(
            "cantalog-test-claude-{}-{}-{}",
            tag,
            std::process::id(),
            n
        ));
        let _ = std::fs::remove_dir_all(&p);
        std::fs::create_dir_all(&p).unwrap();
        p
    }

    fn write_session(path: &std::path::Path, body: &str, mtime: SystemTime) {
        std::fs::write(path, body).unwrap();
        let f = OpenOptions::new().write(true).open(path).unwrap();
        f.set_modified(mtime).unwrap();
    }

    #[test]
    fn string_content_extracted() {
        assert_eq!(
            first_user_message(TYPED_PROMPT).as_deref(),
            Some("How do I set up Tauri v2 with SQLite?")
        );
    }

    #[test]
    fn array_content_first_text_block_extracted() {
        assert_eq!(
            first_user_message(ARRAY_CONTENT).as_deref(),
            Some("Explain borrow checking to a Go developer.")
        );
    }

    #[test]
    fn tool_result_line_is_skipped_in_favor_of_next_real_user_message() {
        assert_eq!(
            first_user_message(TOOL_RESULT_THEN_USER).as_deref(),
            Some("Please explain this error")
        );
    }

    #[test]
    fn malformed_lines_are_skipped() {
        assert_eq!(
            first_user_message(MALFORMED).as_deref(),
            Some("Hello from malformed fixture")
        );
    }

    #[test]
    fn returns_none_when_no_user_message_present() {
        assert_eq!(first_user_message(NO_USER), None);
    }

    #[test]
    fn long_prompt_is_truncated_with_ellipsis() {
        let summary = first_user_message(LONG_PROMPT).expect("summary");
        // 80 chars of body + "…"
        let char_count = summary.chars().count();
        assert_eq!(char_count, 81, "got {summary:?} ({char_count} chars)");
        assert!(summary.ends_with('…'));
        assert!(summary.starts_with("This is a very long prompt"));
    }

    #[test]
    fn truncate_is_char_safe_not_byte_safe() {
        // Multibyte characters shouldn't trigger a panic on char boundary.
        let input = "é".repeat(100);
        let out = truncate_summary(&input, 80);
        assert_eq!(out.chars().count(), 81);
    }

    #[test]
    fn unescape_project_name_follows_spec_literally() {
        assert_eq!(
            unescape_project_name("-Users-josh-code-cantalog"),
            "Users/josh/code/cantalog"
        );
        // Hidden-dir double-dash encoding — spec-literal behavior produces a double slash,
        // which is documented as lossy in a TODO(spec) comment.
        assert_eq!(
            unescape_project_name("-Users-josh--claude"),
            "Users/josh//claude"
        );
    }

    #[test]
    fn is_same_local_day_true_for_same_day() {
        let ref_dt = Local.with_ymd_and_hms(2026, 4, 19, 23, 45, 0).unwrap();
        // Same day at 00:15 local time.
        let morning = Local.with_ymd_and_hms(2026, 4, 19, 0, 15, 0).unwrap();
        assert!(is_same_local_day(SystemTime::from(morning), ref_dt));
    }

    #[test]
    fn is_same_local_day_false_for_different_day() {
        let ref_dt = Local.with_ymd_and_hms(2026, 4, 19, 23, 45, 0).unwrap();
        let yesterday = Local.with_ymd_and_hms(2026, 4, 18, 23, 45, 0).unwrap();
        assert!(!is_same_local_day(SystemTime::from(yesterday), ref_dt));
    }

    #[test]
    fn scan_projects_missing_dir_returns_empty() {
        let dir = std::env::temp_dir().join("cantalog-test-claude-missing");
        let _ = std::fs::remove_dir_all(&dir);
        let reference = Local::now();
        let activities = scan_projects(&dir, reference).unwrap();
        assert!(activities.is_empty());
    }

    #[test]
    fn scan_projects_empty_dir_returns_empty() {
        let base = fresh_temp("empty");
        let activities = scan_projects(&base, Local::now()).unwrap();
        assert!(activities.is_empty());
    }

    #[test]
    fn scan_projects_picks_up_today_session_only() {
        let base = fresh_temp("today");
        let project = base.join("-Users-josh-code-cantalog");
        std::fs::create_dir_all(&project).unwrap();

        let reference = Local.with_ymd_and_hms(2026, 4, 19, 13, 0, 0).unwrap();
        let today_mtime =
            SystemTime::from(Local.with_ymd_and_hms(2026, 4, 19, 10, 12, 0).unwrap());
        let yesterday_mtime =
            SystemTime::from(Local.with_ymd_and_hms(2026, 4, 18, 10, 12, 0).unwrap());

        write_session(&project.join("today.jsonl"), TYPED_PROMPT, today_mtime);
        write_session(&project.join("older.jsonl"), TYPED_PROMPT, yesterday_mtime);

        let activities = scan_projects(&base, reference).unwrap();
        assert_eq!(activities.len(), 1, "got {activities:?}");
        let a = &activities[0];
        assert_eq!(a.source, ActivitySource::ClaudeCodeSession);
        assert_eq!(a.chip, "Users/josh/code/cantalog");
        assert_eq!(a.summary, "How do I set up Tauri v2 with SQLite?");
        assert_eq!(a.id, "claude:today");
        assert!(a.included);
    }

    #[test]
    fn scan_projects_ignores_non_jsonl_files() {
        let base = fresh_temp("non-jsonl");
        let project = base.join("project-x");
        std::fs::create_dir_all(&project).unwrap();
        let today_mtime = SystemTime::now() - Duration::from_secs(60);
        write_session(&project.join("session.jsonl"), TYPED_PROMPT, today_mtime);
        std::fs::write(project.join("notes.md"), "ignored").unwrap();

        let activities = scan_projects(&base, Local::now()).unwrap();
        assert_eq!(activities.len(), 1);
    }
}
