//! API keys and model choice, read from the environment or a `.env` file.
//! Key values never appear in `Debug` output or error messages.

use std::fmt;

use gpui::Global;

/// Used when GEMINI_MODEL isn't set. Models are managed in `.env`.
pub const DEFAULT_GEMINI_MODEL: &str = "gemini-3.1-flash-lite";

#[derive(Clone)]
pub struct Config {
    pub gemini_api_key: Option<String>,
    pub gemini_model: String,
    pub gemini_fallback_model: Option<String>,
    pub serpapi_api_key: Option<String>,
}

impl Global for Config {}

impl Config {
    /// Loads `.env` from the working directory (or, for dev builds, the crate
    /// root), then reads the environment. Real environment variables win.
    pub fn load() -> Self {
        if dotenvy::dotenv().is_err() {
            let _ = dotenvy::from_path(concat!(env!("CARGO_MANIFEST_DIR"), "/.env"));
        }
        Self::from_lookup(|name| std::env::var(name).ok())
    }

    fn from_lookup(lookup: impl Fn(&str) -> Option<String>) -> Self {
        let get = |name: &str| {
            lookup(name)
                .map(|value| value.trim().to_string())
                .filter(|value| !value.is_empty())
        };
        let gemini_model = get("GEMINI_MODEL").unwrap_or_else(|| DEFAULT_GEMINI_MODEL.into());
        // Only a model named in GEMINI_FALLBACK_MODEL is ever used as a fallback.
        let gemini_fallback_model = get("GEMINI_FALLBACK_MODEL")
            .filter(|value| !value.eq_ignore_ascii_case("none"))
            .filter(|fallback| *fallback != gemini_model);
        Self {
            gemini_api_key: get("GEMINI_API_KEY"),
            gemini_model,
            gemini_fallback_model,
            serpapi_api_key: get("SERPAPI_API_KEY"),
        }
    }

    /// Models to try, in order.
    pub fn gemini_models(&self) -> Vec<String> {
        std::iter::once(self.gemini_model.clone())
            .chain(self.gemini_fallback_model.clone())
            .collect()
    }
}

impl fmt::Debug for Config {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let set = |key: &Option<String>| if key.is_some() { "<set>" } else { "<missing>" };
        f.debug_struct("Config")
            .field("gemini_api_key", &set(&self.gemini_api_key))
            .field("gemini_model", &self.gemini_model)
            .field("gemini_fallback_model", &self.gemini_fallback_model)
            .field("serpapi_api_key", &set(&self.serpapi_api_key))
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn blank_values_count_as_missing_and_model_has_a_default() {
        let config = Config::from_lookup(|name| match name {
            "GEMINI_API_KEY" => Some("  secret-value  ".into()),
            "SERPAPI_API_KEY" => Some("   ".into()),
            _ => None,
        });
        assert_eq!(config.gemini_api_key.as_deref(), Some("secret-value"));
        assert_eq!(config.serpapi_api_key, None);
        assert_eq!(config.gemini_model, DEFAULT_GEMINI_MODEL);
        assert_eq!(config.gemini_models(), [DEFAULT_GEMINI_MODEL], "no implicit fallback");
    }

    #[test]
    fn fallback_can_be_disabled_and_is_never_the_primary() {
        let none = Config::from_lookup(|name| (name == "GEMINI_FALLBACK_MODEL").then(|| "None".into()));
        assert_eq!(none.gemini_models(), [DEFAULT_GEMINI_MODEL]);

        let named = Config::from_lookup(|name| match name {
            "GEMINI_MODEL" => Some("gemini-a".into()),
            "GEMINI_FALLBACK_MODEL" => Some("gemini-b".into()),
            _ => None,
        });
        assert_eq!(named.gemini_models(), ["gemini-a", "gemini-b"]);

        let same = Config::from_lookup(|name| match name {
            "GEMINI_MODEL" | "GEMINI_FALLBACK_MODEL" => Some("gemini-x".into()),
            _ => None,
        });
        assert_eq!(same.gemini_models(), ["gemini-x"]);
    }

    #[test]
    fn debug_output_redacts_keys() {
        let config = Config::from_lookup(|name| (name == "GEMINI_API_KEY").then(|| "secret-value".into()));
        let debug = format!("{config:?}");
        assert!(!debug.contains("secret-value"));
        assert!(debug.contains("<set>") && debug.contains("<missing>"));
    }
}
