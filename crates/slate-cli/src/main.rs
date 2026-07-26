use anyhow::Result;
use clap::{Parser, Subcommand};
use tracing_subscriber::EnvFilter;

mod commands;

#[derive(Parser)]
#[command(name = "slate", about = "A terminal info dashboard with plugin ecosystem")]
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
}

#[tokio::main]
async fn main() -> Result<()> {
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
    }
}
