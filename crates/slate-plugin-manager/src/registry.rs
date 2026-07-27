use anyhow::{Context, Result};
use serde::Deserialize;

/// A plugin registry (Homebrew-tap model).
/// Fetched from a GitHub repository containing registry.toml.
#[derive(Debug, Clone)]
pub struct Registry {
    pub url: String,
    pub plugins: Vec<RegistryEntry>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct RegistryEntry {
    pub name: String,
    pub source: String,
    pub description: String,
    #[serde(default)]
    pub tags: Vec<String>,
}

#[derive(Debug, Deserialize)]
struct RegistryFile {
    #[serde(default)]
    plugins: Vec<RegistryEntry>,
}

impl Registry {
    /// Default registry URL.
    pub const DEFAULT_URL: &'static str =
        "https://raw.githubusercontent.com/slate-community/slate-registry/main/registry.toml";

    /// Fetch the registry from a URL.
    pub async fn fetch(url: Option<&str>) -> Result<Self> {
        let url = url.unwrap_or(Self::DEFAULT_URL);
        let client = reqwest::Client::new();
        let content = client
            .get(url)
            .header("User-Agent", "slate-plugin-manager")
            .send()
            .await?
            .text()
            .await?;

        let registry_file: RegistryFile =
            toml::from_str(&content).context("Failed to parse registry.toml")?;

        Ok(Self {
            url: url.to_string(),
            plugins: registry_file.plugins,
        })
    }

    /// Search the registry for plugins matching a query.
    pub fn search(&self, query: &str) -> Vec<&RegistryEntry> {
        let query_lower = query.to_lowercase();
        self.plugins
            .iter()
            .filter(|p| {
                p.name.to_lowercase().contains(&query_lower)
                    || p.description.to_lowercase().contains(&query_lower)
                    || p.tags
                        .iter()
                        .any(|t| t.to_lowercase().contains(&query_lower))
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_registry() -> Registry {
        Registry {
            url: "https://example.test/registry.toml".to_string(),
            plugins: vec![
                RegistryEntry {
                    name: "slate-github".to_string(),
                    source: "github.com/slate-community/slate-github".to_string(),
                    description: "GitHub issues and PRs".to_string(),
                    tags: vec!["github".to_string(), "issues".to_string()],
                },
                RegistryEntry {
                    name: "slate-weather".to_string(),
                    source: "github.com/slate-community/slate-weather".to_string(),
                    description: "Weather forecast widget".to_string(),
                    tags: vec!["weather".to_string(), "forecast".to_string()],
                },
            ],
        }
    }

    #[test]
    fn search_finds_exact_name_matches() {
        let registry = sample_registry();
        let results = registry.search("slate-github");

        assert_eq!(results.len(), 1);
        assert_eq!(results[0].name, "slate-github");
    }

    #[test]
    fn search_finds_partial_matches_in_description_and_tags() {
        let registry = sample_registry();

        let description_results = registry.search("forecast");
        assert_eq!(description_results.len(), 1);
        assert_eq!(description_results[0].name, "slate-weather");

        let tag_results = registry.search("issues");
        assert_eq!(tag_results.len(), 1);
        assert_eq!(tag_results[0].name, "slate-github");
    }

    #[test]
    fn search_returns_no_matches_for_unknown_query() {
        let registry = sample_registry();
        let results = registry.search("kubernetes");

        assert!(results.is_empty());
    }
}
