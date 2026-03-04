//! xpm — Modern package manager for X Distribution
//!
//! Entry point for the xpm binary. Handles CLI parsing, configuration loading,
//! logging initialization, and dispatching to the appropriate subcommand handler.

mod cli;

use anyhow::{Context, Result};
use clap::Parser;
use tracing::Level;
use tracing_subscriber::EnvFilter;

use cli::{Cli, Command};
use xpm_core::XpmConfig;

fn main() -> Result<()> {
    let cli = Cli::parse();

    // ── Initialize logging ──────────────────────────────────────────────
    let log_level = match cli.verbose {
        0 => Level::WARN,
        1 => Level::INFO,
        2 => Level::DEBUG,
        _ => Level::TRACE,
    };

    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::builder()
                .with_default_directive(log_level.into())
                .from_env_lossy(),
        )
        .with_target(false)
        .init();

    tracing::debug!("xpm v{}", env!("CARGO_PKG_VERSION"));

    // ── Load configuration ──────────────────────────────────────────────
    let config_path = cli.config.clone().unwrap_or_else(XpmConfig::default_path);

    let mut config = XpmConfig::load_or_default(&config_path)
        .with_context(|| format!("failed to load config from {}", config_path.display()))?;

    // Apply CLI overrides
    config.apply_overrides(
        cli.root.as_deref(),
        cli.dbpath.as_deref(),
        cli.cachedir.as_deref(),
    );

    if cli.no_color {
        config.options.color = false;
    }

    tracing::info!(
        root = %config.options.root_dir.display(),
        db = %config.options.db_path.display(),
        repos = config.repositories.len(),
        "configuration loaded"
    );

    // ── Dispatch subcommands ────────────────────────────────────────────
    match &cli.command {
        Command::Sync(args) => cmd_sync(&config, args),
        Command::Install(args) => cmd_install(&config, args, cli.no_confirm),
        Command::Remove(args) => cmd_remove(&config, args, cli.no_confirm),
        Command::Upgrade(args) => cmd_upgrade(&config, args, cli.no_confirm),
        Command::Query(args) => cmd_query(&config, args),
        Command::Search(args) => cmd_search(&config, args),
        Command::Info(args) => cmd_info(&config, args),
        Command::Files(args) => cmd_files(&config, args),
    }
}

// ── Subcommand stubs ────────────────────────────────────────────────────────
//
// Each function below is a placeholder that will be filled with real logic
// in subsequent phases. For now they confirm the CLI pipeline works end-to-end.

fn cmd_sync(config: &XpmConfig, args: &cli::SyncArgs) -> Result<()> {
    let force = if args.force { " (forced)" } else { "" };
    println!(":: Synchronizing package databases{force}...");
    for repo in &config.repositories {
        println!("   {} — {} server(s)", repo.name, repo.server.len());
    }
    println!(":: Sync complete (stub).");
    Ok(())
}

fn cmd_install(_config: &XpmConfig, args: &cli::InstallArgs, no_confirm: bool) -> Result<()> {
    println!(
        ":: Resolving dependencies for: {}",
        args.packages.join(", ")
    );
    if args.download_only {
        println!("   (download only mode)");
    }
    if !no_confirm {
        println!(":: Proceed with installation? [Y/n] (auto-confirmed in stub)");
    }
    println!(":: Installation complete (stub).");
    Ok(())
}

fn cmd_remove(_config: &XpmConfig, args: &cli::RemoveArgs, _no_confirm: bool) -> Result<()> {
    println!(":: Removing packages: {}", args.packages.join(", "));
    if args.recursive {
        println!("   (including unneeded dependencies)");
    }
    println!(":: Removal complete (stub).");
    Ok(())
}

fn cmd_upgrade(_config: &XpmConfig, args: &cli::UpgradeArgs, _no_confirm: bool) -> Result<()> {
    println!(":: Starting full system upgrade...");
    if !args.ignore.is_empty() {
        println!("   ignoring: {}", args.ignore.join(", "));
    }
    println!(":: Upgrade complete (stub).");
    Ok(())
}

fn cmd_query(_config: &XpmConfig, args: &cli::QueryArgs) -> Result<()> {
    let filter_type = if args.explicit {
        "explicitly installed"
    } else if args.deps {
        "dependency"
    } else if args.orphans {
        "orphan"
    } else if args.upgrades {
        "upgradeable"
    } else {
        "all"
    };
    println!(":: Querying {filter_type} packages...");
    if let Some(ref f) = args.filter {
        println!("   filter: {f}");
    }
    println!(":: Query complete (stub).");
    Ok(())
}

fn cmd_search(_config: &XpmConfig, args: &cli::SearchArgs) -> Result<()> {
    let db = if args.local { "local" } else { "sync" };
    println!(":: Searching {db} database for '{}'...", args.query);
    println!(":: Search complete (stub).");
    Ok(())
}

fn cmd_info(_config: &XpmConfig, args: &cli::InfoArgs) -> Result<()> {
    let db = if args.local { "local" } else { "sync" };
    println!(":: Package info ({db}): {}", args.package);
    println!(":: Info complete (stub).");
    Ok(())
}

fn cmd_files(_config: &XpmConfig, args: &cli::FilesArgs) -> Result<()> {
    println!(":: Files owned by '{}':", args.package);
    println!(":: File listing complete (stub).");
    Ok(())
}
