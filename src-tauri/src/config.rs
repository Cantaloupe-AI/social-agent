use crate::types::{ClaudeCodeConfig, Config, GitConfig};
use anyhow::{anyhow, Context, Result};
use std::path::{Path, PathBuf};

pub const CONFIG_FILENAME: &str = "config.toml";

pub fn default_base_dir() -> Result<PathBuf> {
    let dir = dirs::data_dir()
        .ok_or_else(|| anyhow!("could not resolve OS data directory"))?
        .join("cantalog");
    Ok(dir)
}

pub fn default_config() -> Config {
    let projects_dir = dirs::home_dir()
        .map(|h| h.join(".claude/projects"))
        .unwrap_or_else(|| PathBuf::from(".claude/projects"));
    Config {
        git: GitConfig::default(),
        claude_code: ClaudeCodeConfig { projects_dir },
    }
}

pub fn load_config_from(base_dir: &Path) -> Result<Config> {
    let path = base_dir.join(CONFIG_FILENAME);
    if !path.exists() {
        let cfg = default_config();
        save_config_to(base_dir, &cfg)?;
        return Ok(cfg);
    }
    let contents = std::fs::read_to_string(&path)
        .with_context(|| format!("reading config from {}", path.display()))?;
    let cfg: Config = toml::from_str(&contents)
        .with_context(|| format!("parsing config at {}", path.display()))?;
    Ok(cfg)
}

pub fn save_config_to(base_dir: &Path, cfg: &Config) -> Result<()> {
    std::fs::create_dir_all(base_dir)
        .with_context(|| format!("creating config dir {}", base_dir.display()))?;
    let path = base_dir.join(CONFIG_FILENAME);
    let contents =
        toml::to_string_pretty(cfg).context("serializing config to TOML")?;
    std::fs::write(&path, contents)
        .with_context(|| format!("writing config to {}", path.display()))?;
    Ok(())
}

pub fn load_config() -> Result<Config> {
    load_config_from(&default_base_dir()?)
}

pub fn save_config(cfg: &Config) -> Result<()> {
    save_config_to(&default_base_dir()?, cfg)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU32, Ordering};

    static COUNTER: AtomicU32 = AtomicU32::new(0);

    fn fresh_base_dir(tag: &str) -> PathBuf {
        let n = COUNTER.fetch_add(1, Ordering::SeqCst);
        let dir = std::env::temp_dir().join(format!(
            "cantalog-test-config-{}-{}-{}",
            tag,
            std::process::id(),
            n
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn creates_defaults_when_missing() {
        let base = fresh_base_dir("defaults");
        let cfg = load_config_from(&base).expect("loaded");
        assert!(cfg.git.repos.is_empty());
        assert!(cfg
            .claude_code
            .projects_dir
            .ends_with(".claude/projects"));
        assert!(base.join(CONFIG_FILENAME).exists());
    }

    #[test]
    fn save_and_reload_roundtrip() {
        let base = fresh_base_dir("roundtrip");
        let mut cfg = default_config();
        cfg.git.repos.push(PathBuf::from("/Users/test/repo-a"));
        cfg.git.repos.push(PathBuf::from("/Users/test/repo-b"));
        cfg.claude_code.projects_dir = PathBuf::from("/custom/projects");
        save_config_to(&base, &cfg).expect("saved");

        let loaded = load_config_from(&base).expect("reloaded");
        assert_eq!(cfg, loaded);
    }

    #[test]
    fn accepts_config_without_git_section() {
        let base = fresh_base_dir("no-git");
        std::fs::create_dir_all(&base).unwrap();
        std::fs::write(
            base.join(CONFIG_FILENAME),
            r#"
[claude_code]
projects_dir = "/tmp/foo"
"#,
        )
        .unwrap();
        let cfg = load_config_from(&base).expect("loaded");
        assert!(cfg.git.repos.is_empty());
        assert_eq!(cfg.claude_code.projects_dir, PathBuf::from("/tmp/foo"));
    }
}
