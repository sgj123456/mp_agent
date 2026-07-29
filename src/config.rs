use serde::Deserialize;
use std::env;
use std::path::{Path, PathBuf};

/// The default config file name (part of the path, not full).
const CONFIG_FILE: &str = "config.toml";

#[derive(Debug, Clone)]
pub struct Config {
    pub api_key: String,
    pub base_url: String,
    pub model: String,
    pub max_tokens: Option<u32>,
}

/// Find the first existing config file: project → global user config.
pub fn find_config_path() -> Option<PathBuf> {
    let project = Path::new(".mp_agent").join(CONFIG_FILE);
    if project.exists() {
        return Some(project);
    }
    global_config_dir()
        .map(|d| d.join(CONFIG_FILE))
        .filter(|p| p.exists())
}

/// Return the global mp_agent config directory (e.g. `~/.config/mp_agent/` on Linux,
/// `%APPDATA%/mp_agent/` on Windows).
pub fn global_config_dir() -> Option<PathBuf> {
    dirs::config_dir().map(|d| d.join("mp_agent"))
}

/// Generate a default config file at the global path if none exists.
/// Also creates the global config directory and skills directory.
fn generate_default_config(path: &Path) -> color_eyre::Result<()> {
    if path.exists() {
        return Ok(());
    }
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| color_eyre::eyre::eyre!("Failed to create config dir: {}", e))?;
        // Also create the global skills directory alongside config
        let skills_dir = parent.join("skills");
        let _ = std::fs::create_dir_all(&skills_dir);
    }
    let template = r#"# mp_agent configuration
#
# Get your API key from your OpenAI-compatible provider.
# Then edit the value below and save this file.

[api]
api_key = "your-api-key-here"
base_url = "https://api.openai.com/v1"
model = "gpt-4o"
# max_tokens = 4096

# Uncomment and configure MCP servers below:
# [mcp.servers]
# [mcp.servers.example]
# command = "npx"
# args = ["-y", "@modelcontextprotocol/server-filesystem", "/tmp"]
# enabled = true
"#;
    std::fs::write(path, template)
        .map_err(|e| color_eyre::eyre::eyre!("Failed to create {}: {}", path.display(), e))?;
    tracing::info!("Generated default config at {}", path.display());
    Ok(())
}

impl Config {
    /// Load config from project or global TOML, auto-generating a default
    /// global config if neither exists. Falls back to `.env` as a last resort.
    pub fn load() -> color_eyre::Result<Self> {
        if let Some(config_path) = find_config_path() {
            let contents = std::fs::read_to_string(&config_path).map_err(|e| {
                color_eyre::eyre::eyre!("Failed to read {}: {}", config_path.display(), e)
            })?;
            return Self::from_toml_str(&contents);
        }
        // No config file exists — generate a default at the global location.
        if let Some(global_dir) = global_config_dir() {
            let global_path = global_dir.join(CONFIG_FILE);
            if let Err(e) = generate_default_config(&global_path) {
                tracing::warn!("Could not generate default config: {}", e);
            } else {
                let contents = std::fs::read_to_string(&global_path).map_err(|e| {
                    color_eyre::eyre::eyre!("Failed to read {}: {}", global_path.display(), e)
                })?;
                return Self::from_toml_str(&contents);
            }
        }
        Self::from_env()
    }

    fn from_toml_str(toml_str: &str) -> color_eyre::Result<Self> {
        #[derive(Deserialize)]
        struct TomlConfig {
            api: ApiConfig,
        }
        #[derive(Deserialize)]
        struct ApiConfig {
            api_key: String,
            base_url: Option<String>,
            model: Option<String>,
            max_tokens: Option<u32>,
        }
        let tc: TomlConfig = toml::from_str(toml_str)
            .map_err(|e| color_eyre::eyre::eyre!("Failed to parse config.toml: {}", e))?;
        Ok(Config {
            api_key: tc.api.api_key,
            base_url: tc
                .api
                .base_url
                .unwrap_or_else(|| "https://api.openai.com/v1".to_string())
                .trim_end_matches('/')
                .to_string(),
            model: tc.api.model.unwrap_or_else(|| "gpt-4o".to_string()),
            max_tokens: tc.api.max_tokens,
        })
    }

    pub fn from_env() -> color_eyre::Result<Self> {
        dotenvy::dotenv().ok();
        Self::from_env_vars()
    }

    /// Read config from environment variables directly (no dotenv reload).
    pub(crate) fn from_env_vars() -> color_eyre::Result<Self> {
        let api_key = env::var("OPENAI_API_KEY")
            .map_err(|_| color_eyre::eyre::eyre!("OPENAI_API_KEY not set in .env"))?;

        let base_url = env::var("OPENAI_BASE_URL")
            .unwrap_or_else(|_| "https://api.openai.com/v1".to_string())
            .trim_end_matches('/')
            .to_string();

        let model = env::var("OPENAI_MODEL").unwrap_or_else(|_| "gpt-4o".to_string());

        let max_tokens = env::var("OPENAI_MAX_TOKENS")
            .ok()
            .and_then(|v| v.parse().ok());

        tracing::info!(
            "Config loaded: model={}, base_url={}, max_tokens={:?}",
            model,
            base_url,
            max_tokens
        );

        Ok(Config {
            api_key,
            base_url,
            model,
            max_tokens,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    static ENV_LOCK: Mutex<()> = Mutex::new(());

    fn with_clean_env<F: FnOnce()>(f: F) {
        let _lock = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        unsafe {
            let old_key = std::env::var("OPENAI_API_KEY").ok();
            let old_url = std::env::var("OPENAI_BASE_URL").ok();
            let old_model = std::env::var("OPENAI_MODEL").ok();
            let old_tokens = std::env::var("OPENAI_MAX_TOKENS").ok();
            std::env::remove_var("OPENAI_API_KEY");
            std::env::remove_var("OPENAI_BASE_URL");
            std::env::remove_var("OPENAI_MODEL");
            std::env::remove_var("OPENAI_MAX_TOKENS");
            f();
            if let Some(v) = old_key {
                std::env::set_var("OPENAI_API_KEY", v);
            }
            if let Some(v) = old_url {
                std::env::set_var("OPENAI_BASE_URL", v);
            }
            if let Some(v) = old_model {
                std::env::set_var("OPENAI_MODEL", v);
            }
            if let Some(v) = old_tokens {
                std::env::set_var("OPENAI_MAX_TOKENS", v);
            }
        }
    }

    #[test]
    fn test_config_from_env_with_env_vars() {
        with_clean_env(|| {
            unsafe {
                std::env::set_var("OPENAI_API_KEY", "sk-test-key");
                std::env::set_var("OPENAI_BASE_URL", "https://custom.example.com/v1");
                std::env::set_var("OPENAI_MODEL", "test-model");
                std::env::set_var("OPENAI_MAX_TOKENS", "4096");
            }
            let config = Config::from_env_vars().unwrap();
            assert_eq!(config.api_key, "sk-test-key");
            assert_eq!(config.base_url, "https://custom.example.com/v1");
            assert_eq!(config.model, "test-model");
            assert_eq!(config.max_tokens, Some(4096));
        });
    }

    #[test]
    fn test_config_default_base_url_and_model() {
        with_clean_env(|| {
            unsafe {
                std::env::set_var("OPENAI_API_KEY", "sk-test-key");
            }
            let config = Config::from_env_vars().unwrap();
            assert_eq!(config.base_url, "https://api.openai.com/v1");
            assert_eq!(config.model, "gpt-4o");
            assert_eq!(config.max_tokens, None);
        });
    }

    #[test]
    fn test_config_base_url_trims_trailing_slash() {
        with_clean_env(|| {
            unsafe {
                std::env::set_var("OPENAI_API_KEY", "sk-test-key");
                std::env::set_var("OPENAI_BASE_URL", "https://api.example.com/v1/");
            }
            let config = Config::from_env_vars().unwrap();
            assert_eq!(config.base_url, "https://api.example.com/v1");
        });
    }

    #[test]
    fn test_config_max_tokens_parse_optional() {
        with_clean_env(|| {
            unsafe {
                std::env::set_var("OPENAI_API_KEY", "sk-test-key");
                std::env::set_var("OPENAI_MAX_TOKENS", "invalid");
            }
            let config = Config::from_env_vars().unwrap();
            assert_eq!(config.max_tokens, None);
        });
    }
}
