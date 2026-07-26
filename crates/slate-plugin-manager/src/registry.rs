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
                    || p.tags.iter().any(|t| t.to_lowercase().contains(&query_lower))
            })
            .collect()
    }
}
