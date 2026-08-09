use anyhow::Result;
use clap::{Parser, Subcommand};
use tracing_subscriber::{fmt, layer::SubscriberExt, util::SubscriberInitExt, EnvFilter};

mod builtins;
mod commands;
mod docs;

/// Get the slate config directory path.
fn dirs_config_dir() -> Option<std::path::PathBuf> {
    #[cfg(target_os = "windows")]
    {
        std::env::var("APPDATA")
            .ok()
            .map(|p| std::path::PathBuf::from(p).join("slate"))
    }
    #[cfg(not(target_os = "windows"))]
    {
        std::env::var("XDG_CONFIG_HOME")
            .ok()
            .map(std::path::PathBuf::from)
            .or_else(|| {
                std::env::var("HOME")
                    .ok()
                    .map(|h| std::path::PathBuf::from(h).join(".config"))
            })
            .map(|p| p.join("slate"))
    }
}

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
    /// Serve a read-only dashboard over HTTP
    Serve {
        /// Path to config file
        #[arg(short, long)]
        config: Option<String>,
        /// Host interface to bind
        #[arg(long, default_value = "127.0.0.1")]
        host: String,
        /// TCP port to bind
        #[arg(long, default_value_t = 8787)]
        port: u16,
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
    /// Lint a plugin directory for baseline compliance
    Lint {
        /// Path to plugin directory (default: current directory)
        path: Option<String>,
        /// Auto-generate [config] section from source code
        #[arg(long)]
        fix: bool,
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

    let cli = Cli::parse();

    // Set up logging: file-based for dashboard, stderr for CLI commands
    let is_dashboard = matches!(cli.command, None | Some(Commands::Run { .. }));
    if is_dashboard {
        // Log to ~/.config/slate/slate.log (or %APPDATA%/slate/slate.log on Windows)
        let log_dir = dirs_config_dir();
        if let Some(dir) = &log_dir {
            let _ = std::fs::create_dir_all(dir);
        }
        let log_path = log_dir
            .map(|d| d.join("slate.log"))
            .unwrap_or_else(|| std::path::PathBuf::from("slate.log"));
        let file = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&log_path)
            .ok();
        if let Some(file) = file {
            let file_layer = fmt::layer()
                .with_writer(std::sync::Mutex::new(file))
                .with_ansi(false);
            tracing_subscriber::registry()
                .with(EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("warn")))
                .with(file_layer)
                .init();
        } else {
            tracing_subscriber::fmt()
                .with_env_filter(EnvFilter::from_default_env())
                .init();
        }
    } else {
        tracing_subscriber::fmt()
            .with_env_filter(EnvFilter::from_default_env())
            .init();
    }

    match cli.command {
        None | Some(Commands::Run { .. }) => {
            let config_path = match &cli.command {
                Some(Commands::Run { config: Some(p) }) => Some(p.as_str()),
                _ => None,
            };
            commands::run(config_path).await
        }
        Some(Commands::Serve { config, host, port }) => {
            commands::serve(config.as_deref(), &host, port).await
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
        Some(Commands::Lint { path, fix }) => {
            if fix {
                let dir = path
                    .map(std::path::PathBuf::from)
                    .unwrap_or_else(|| std::env::current_dir().unwrap());
                commands::lint_fix(&dir)
            } else {
                commands::lint(path.as_deref()).await
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cli_parses_default_run_and_named_subcommands() {
        let cli = Cli::parse_from(["slate"]);
        assert!(cli.command.is_none());

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

        let cli = Cli::parse_from(["slate", "serve", "--host", "0.0.0.0", "--port", "9000"]);
        assert!(matches!(
            cli.command,
            Some(Commands::Serve { host, port, .. }) if host == "0.0.0.0" && port == 9000
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
