use anyhow::{Context, Result};
use regex::Regex;
use std::path::{Path, PathBuf};
use toml::Value;

#[derive(Debug, Clone, Default)]
pub struct Config {
    sticky_rules: Vec<CompiledRule>,
}

#[derive(Debug, Clone)]
struct CompiledRule {
    app_id: Option<Regex>,
    title: Option<Regex>,
}

impl Config {
    pub fn load<P: AsRef<Path>>(path: P) -> Result<Self> {
        let path = path.as_ref();
        let content = std::fs::read_to_string(path)
            .with_context(|| format!("Failed to read config: {path:?}"))?;

        let value: Value = content
            .parse()
            .with_context(|| format!("Failed to parse TOML: {path:?}"))?;

        let table = match value {
            Value::Table(t) => t,
            _ => return Ok(Config::default()),
        };

        let mut sticky_rules = Vec::new();

        if let Some(sticky) = table.get("sticky")
            && let Some(sticky_table) = sticky.as_table()
        {
            for (name, value) in sticky_table {
                if let Some(rule_table) = value.as_table() {
                    let app_id = rule_table
                        .get("app_id")
                        .and_then(|v| v.as_str())
                        .map(String::from);
                    let title = rule_table
                        .get("title")
                        .and_then(|v| v.as_str())
                        .map(String::from);

                    if app_id.is_none() && title.is_none() {
                        continue;
                    }

                    let compiled_app_id = app_id
                        .as_ref()
                        .map(|p| {
                            Regex::new(p)
                                .with_context(|| format!("Invalid app_id regex in sticky.{name}"))
                        })
                        .transpose()?;

                    let compiled_title = title
                        .as_ref()
                        .map(|p| {
                            Regex::new(p)
                                .with_context(|| format!("Invalid title regex in sticky.{name}"))
                        })
                        .transpose()?;

                    sticky_rules.push(CompiledRule {
                        app_id: compiled_app_id,
                        title: compiled_title,
                    });
                }
            }
        }

        Ok(Config { sticky_rules })
    }

    pub fn default_config_dir() -> PathBuf {
        dirs::config_dir()
            .unwrap_or_else(|| PathBuf::from("/tmp/nsticky"))
            .join("nsticky")
    }

    pub fn default_config_path() -> PathBuf {
        Self::default_config_dir().join("config.toml")
    }

    pub fn load_or_default() -> Self {
        let path = Self::default_config_path();
        match Self::load(&path) {
            Ok(config) => config,
            Err(e) => {
                tracing::warn!("Failed to load config: {e}, using default (no rules)");
                Config::default()
            }
        }
    }

    pub fn match_sticky(&self, app_id: &Option<String>, title: &Option<String>) -> bool {
        if self.sticky_rules.is_empty() {
            return false;
        }
        for rule in &self.sticky_rules {
            let app_match = match (&rule.app_id, app_id) {
                (None, _) => true,
                (Some(_), None) => false,
                (Some(re), Some(id)) => re.is_match(id),
            };

            let title_match = match (&rule.title, title) {
                (None, _) => true,
                (Some(_), None) => false,
                (Some(re), Some(t)) => re.is_match(t),
            };

            if app_match && title_match {
                return true;
            }
        }
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct TempDir(std::path::PathBuf);
    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    fn temp_config(content: &str) -> (Config, TempDir) {
        let id = std::thread::current().id();
        let dir = std::env::temp_dir().join(format!("nsticky_test_{id:?}"));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("config.toml");
        std::fs::write(&path, content).unwrap();
        let config = match Config::load(&path) {
            Ok(c) => c,
            Err(e) => {
                eprintln!("Config load error: {e}");
                Config::default()
            }
        };
        (config, TempDir(dir))
    }

    #[test]
    fn test_match_app_id_only() {
        let (config, _dir) = temp_config(
            r#"
[sticky.firefox]
app_id = "firefox"
"#,
        );
        assert!(config.match_sticky(&Some("firefox".to_string()), &None));
        assert!(!config.match_sticky(&Some("chrome".to_string()), &None));
    }

    #[test]
    fn test_match_title_only() {
        let (config, _dir) = temp_config(
            r#"
[sticky.gmail]
title = "Gmail"
"#,
        );
        assert!(config.match_sticky(&None, &Some("Inbox - Gmail".to_string())));
        assert!(!config.match_sticky(&None, &Some("YouTube".to_string())));
    }

    #[test]
    fn test_match_both_and() {
        let (config, _dir) = temp_config(
            r#"
[sticky.firefox-gmail]
app_id = "firefox"
title = "Gmail"
"#,
        );
        assert!(config.match_sticky(
            &Some("firefox".to_string()),
            &Some("Inbox - Gmail".to_string())
        ));
        assert!(!config.match_sticky(&Some("firefox".to_string()), &Some("YouTube".to_string())));
        assert!(!config.match_sticky(&Some("chrome".to_string()), &Some("Gmail".to_string())));
    }

    #[test]
    fn test_multiple_rules() {
        let (config, _dir) = temp_config(
            r#"
[sticky.firefox]
app_id = "firefox"

[sticky.chromium]
app_id = "chromium"
"#,
        );
        assert!(config.match_sticky(&Some("firefox".to_string()), &None));
        assert!(config.match_sticky(&Some("chromium".to_string()), &None));
        assert!(!config.match_sticky(&Some("chrome".to_string()), &None));
    }

    #[test]
    fn test_no_rules() {
        let (config, _dir) = temp_config("");
        assert!(!config.match_sticky(&Some("firefox".to_string()), &Some("test".to_string())));
    }

    #[test]
    fn test_missing_file_returns_err() {
        let result = Config::load("/tmp/nsticky_nonexistent_test_file_12345.toml");
        assert!(result.is_err());
    }

    #[test]
    fn test_empty_file_returns_default() {
        let (config, _dir) = temp_config("");
        assert!(!config.match_sticky(&Some("firefox".to_string()), &Some("test".to_string())));
    }

    #[test]
    fn test_unknown_fields_are_ignored_gracefully() {
        let (config, _dir) = temp_config(
            r#"
[sticky.firefox]
app_id = "firefox"
unknown_field = "should_be_ignored"

[unrelated_table]
some_key = "also_ignored"
"#,
        );
        assert!(config.match_sticky(&Some("firefox".to_string()), &None));
        assert!(!config.match_sticky(&Some("chrome".to_string()), &None));
    }

    #[test]
    fn test_invalid_regex() {
        let id = std::thread::current().id();
        let dir = std::env::temp_dir().join(format!("nsticky_test_{id:?}"));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("config.toml");
        std::fs::write(
            &path,
            r#"
[sticky.test]
app_id = "[invalid"
"#,
        )
        .unwrap();
        let result = Config::load(&path);
        let _dir = TempDir(dir);
        assert!(result.is_err());
    }
}
