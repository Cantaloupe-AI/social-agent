use crate::types::{Activity, ActivitySource};
use anyhow::Result;
use chrono::{DateTime, Utc};
use std::path::Path;
use std::process::Command;

/// Parses git log output of the form `<sha>\t<ISO 8601>\t<subject>`, one commit per line.
/// Lines that don't have exactly three tab-separated fields or whose timestamp won't parse
/// are skipped. `repo_name` is attached to every activity as its `chip`.
pub fn parse_git_log(output: &str, repo_name: &str) -> Vec<Activity> {
    let mut activities = Vec::new();
    for line in output.lines() {
        let line = line.trim_end_matches('\r');
        if line.is_empty() {
            continue;
        }
        let mut parts = line.splitn(3, '\t');
        let Some(hash) = parts.next() else { continue };
        let Some(timestamp_raw) = parts.next() else {
            continue;
        };
        let Some(subject) = parts.next() else {
            continue;
        };
        if hash.is_empty() || subject.is_empty() {
            continue;
        }
        let Ok(timestamp) = DateTime::parse_from_rfc3339(timestamp_raw) else {
            continue;
        };
        activities.push(Activity {
            id: format!("git:{repo_name}:{hash}"),
            source: ActivitySource::GitCommit,
            timestamp: timestamp.with_timezone(&Utc),
            summary: subject.to_string(),
            chip: repo_name.to_string(),
            included: true,
        });
    }
    activities
}

/// Shells out to `git log` for commits made since midnight local time in `repo_path`.
/// On any failure (not a git repo, binary missing, permission denied), logs to stderr
/// and returns an empty vec so one bad repo doesn't break the whole fetch.
pub fn fetch_today(repo_path: &Path) -> Result<Vec<Activity>> {
    let repo_name = repo_path
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or("unknown")
        .to_string();

    let output = match Command::new("git")
        .args([
            "-C",
            &repo_path.to_string_lossy(),
            "log",
            "--since=midnight",
            "--pretty=format:%H%x09%aI%x09%s",
            "--no-merges",
        ])
        .output()
    {
        Ok(o) => o,
        Err(e) => {
            eprintln!(
                "cantalog: git log failed for {}: {}",
                repo_path.display(),
                e
            );
            return Ok(Vec::new());
        }
    };

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        eprintln!(
            "cantalog: git log non-zero exit for {}: {}",
            repo_path.display(),
            stderr.trim()
        );
        return Ok(Vec::new());
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    Ok(parse_git_log(&stdout, &repo_name))
}

#[cfg(test)]
mod tests {
    use super::*;

    const EMPTY: &str = include_str!("../../tests/fixtures/git/empty.txt");
    const SINGLE: &str = include_str!("../../tests/fixtures/git/single.txt");
    const MULTI: &str = include_str!("../../tests/fixtures/git/multi.txt");
    const WITH_BLANKS: &str = include_str!("../../tests/fixtures/git/with_blank_lines.txt");

    #[test]
    fn empty_output_yields_empty_vec() {
        assert!(parse_git_log(EMPTY, "cantalog").is_empty());
    }

    #[test]
    fn single_commit_is_parsed() {
        let activities = parse_git_log(SINGLE, "cantalog");
        assert_eq!(activities.len(), 1);
        let a = &activities[0];
        assert_eq!(a.source, ActivitySource::GitCommit);
        assert_eq!(a.chip, "cantalog");
        assert_eq!(a.summary, "feat(db): add entries table migration");
        assert!(a.id.starts_with("git:cantalog:a1b2c3d4"));
        assert!(a.included);
        // 14:32 PDT is 21:32 UTC.
        assert_eq!(a.timestamp.to_rfc3339(), "2026-04-19T21:32:11+00:00");
    }

    #[test]
    fn multi_commit_preserves_log_order() {
        let activities = parse_git_log(MULTI, "cantalog");
        assert_eq!(activities.len(), 3);
        assert_eq!(activities[0].summary, "feat(db): add entries table migration");
        assert_eq!(activities[1].summary, "fix: handle empty git log output");
        assert_eq!(activities[2].summary, "chore: rename project to cantalog");
    }

    #[test]
    fn blank_lines_are_skipped() {
        let activities = parse_git_log(WITH_BLANKS, "cantalog");
        assert_eq!(activities.len(), 2);
    }

    #[test]
    fn malformed_lines_are_skipped() {
        let raw = "not a real git line\nalso garbage\t\n\
                   a1b2c3d4e5f6a1b2c3d4e5f6a1b2c3d4e5f6a1b2\t2026-04-19T14:32:11-07:00\tfeat: ok\n";
        let activities = parse_git_log(raw, "repo");
        assert_eq!(activities.len(), 1);
        assert_eq!(activities[0].summary, "feat: ok");
    }

    #[test]
    fn fetch_today_returns_empty_for_non_repo() {
        let non_repo = std::env::temp_dir().join("cantalog-test-not-a-repo");
        std::fs::create_dir_all(&non_repo).unwrap();
        let activities = fetch_today(&non_repo).unwrap();
        assert!(activities.is_empty());
    }
}
