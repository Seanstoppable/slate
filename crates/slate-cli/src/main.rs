use anyhow::Result;
use clap::{Parser, Subcommand};
use tracing_subscriber::EnvFilter;

mod builtins;
mod commands;
mod docs;

#[derive(Parser)]
#[command(
    name = "slate",
    about = "A terminal info dashboard with plugin ecosystem"
)]
#[command(version, long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: Option<Commands>,
}

#[derive(Subcommand)]
enum Commands {
    /// Launch the dashboard
    Run {
        /// Path to config file
        #[arg(short, long)]
        config: Option<String>,
    },
    /// Install all declared plugins
    Install,
    /// Update plugins to latest compatible versions
    Update,
    /// Show available plugin updates
    Outdated,
    /// List installed plugins
    List,
    /// Remove a plugin
    Remove {
        /// Plugin name to remove
        name: String,
    },
    /// Scaffold a new plugin project
    Create {
        /// Plugin name
        name: String,
    },
    /// Search the plugin registry
    Search {
        /// Search query
        query: String,
    },
    /// Convert a wtfutil config to Slate format
    Migrate {
        /// Path to wtfutil YAML config
        path: String,
    },
    /// Validate config and all plugins without launching the dashboard
    Check {
        /// Path to config file
        #[arg(short, long)]
        config: Option<String>,
    },
    /// Generate plugin documentation website
    Docs {
        /// Output directory (default: docs/plugins)
        #[arg(short, long)]
        output: Option<String>,
    },
}

#[tokio::main]
async fn main() -> Result<()> {
    // Install panic hook that restores terminal before printing panic
    let default_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let _ = crossterm::terminal::disable_raw_mode();
        let _ = crossterm::execute!(
            std::io::stdout(),
            crossterm::cursor::Show,
            crossterm::terminal::LeaveAlternateScreen
        );
        default_hook(info);
    }));

    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env())
        .init();

    let cli = Cli::parse();

    match cli.command {
        None | Some(Commands::Run { .. }) => {
            let config_path = match &cli.command {
                Some(Commands::Run { config: Some(p) }) => Some(p.as_str()),
                _ => None,
            };
            commands::run(config_path).await
        }
        Some(Commands::Install) => commands::install().await,
        Some(Commands::Update) => commands::update().await,
        Some(Commands::Outdated) => commands::outdated().await,
        Some(Commands::List) => commands::list().await,
        Some(Commands::Remove { name }) => commands::remove(&name).await,
        Some(Commands::Create { name }) => commands::create(&name).await,
        Some(Commands::Search { query }) => commands::search(&query).await,
        Some(Commands::Migrate { path }) => commands::migrate(&path).await,
        Some(Commands::Check { config }) => commands::check(config.as_deref()).await,
        Some(Commands::Docs { output }) => docs::docs(output.as_deref()).await,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cli_parses_default_run_and_named_subcommands() {
        let cli = Cli::parse_from(["slate"]);
        assert!(matches!(cli.command, None));

        let cli = Cli::parse_from(["slate", "run", "--config", "slate.toml"]);
        assert!(matches!(
            cli.command,
            Some(Commands::Run { config: Some(path) }) if path == "slate.toml"
        ));

        let cli = Cli::parse_from(["slate", "docs", "--output", "site"]);
        assert!(matches!(
            cli.command,
            Some(Commands::Docs { output: Some(path) }) if path == "site"
        ));
    }

    #[test]
    fn cli_parses_management_commands() {
        let cli = Cli::parse_from(["slate", "remove", "clock"]);
        assert!(matches!(
            cli.command,
            Some(Commands::Remove { name }) if name == "clock"
        ));

        let cli = Cli::parse_from(["slate", "search", "github"]);
        assert!(matches!(
            cli.command,
            Some(Commands::Search { query }) if query == "github"
        ));

        let cli = Cli::parse_from(["slate", "check", "--config", "custom.toml"]);
        assert!(matches!(
            cli.command,
            Some(Commands::Check { config: Some(path) }) if path == "custom.toml"
        ));
    }
}
