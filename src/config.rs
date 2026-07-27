use std::env;

#[derive(Debug, Clone)]
pub struct Config {
    pub api_key: String,
    pub base_url: String,
    pub model: String,
    pub max_tokens: Option<u32>,
}

impl Config {
    pub fn from_env() -> color_eyre::Result<Self> {
        dotenvy::dotenv().ok();

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

    fn from_env_isolated() -> color_eyre::Result<Config> {
        let api_key = env::var("OPENAI_API_KEY")
            .map_err(|_| color_eyre::eyre::eyre!("OPENAI_API_KEY not set"))?;
        let base_url = env::var("OPENAI_BASE_URL")
            .unwrap_or_else(|_| "https://api.openai.com/v1".to_string())
            .trim_end_matches('/')
            .to_string();
        let model = env::var("OPENAI_MODEL").unwrap_or_else(|_| "gpt-4o".to_string());
        let max_tokens = env::var("OPENAI_MAX_TOKENS")
            .ok()
            .and_then(|v| v.parse::<u32>().ok());
        Ok(Config {
            api_key,
            base_url,
            model,
            max_tokens,
        })
    }

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
            let config = from_env_isolated().unwrap();
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
            let config = from_env_isolated().unwrap();
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
            let config = from_env_isolated().unwrap();
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
            let config = from_env_isolated().unwrap();
            assert_eq!(config.max_tokens, None);
        });
    }
}
