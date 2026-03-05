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
use xpm_core::repo::RepoManager;
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
        Command::Repo(args) => cmd_repo(&config, args),
        Command::Usage(args) => cmd_help(args),
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

fn cmd_repo(config: &XpmConfig, args: &cli::RepoArgs) -> Result<()> {
    let manager = RepoManager::default_dir();

    match &args.action {
        cli::RepoAction::Add(add) => {
            manager
                .add(&add.name, &add.url)
                .with_context(|| format!("failed to add repository '{}'", add.name))?;
            println!(":: Repository '{}' added successfully.", add.name);
            println!("   url: {}", add.url);
            println!("   Run 'xpm sync' to refresh databases.");
        }
        cli::RepoAction::Remove(rm) => {
            manager
                .remove(&rm.name)
                .with_context(|| format!("failed to remove repository '{}'", rm.name))?;
            println!(":: Repository '{}' removed.", rm.name);
        }
        cli::RepoAction::List => {
            println!(":: Active repositories:");
            println!();

            // Predefined repos from config
            println!("   [predefined]");
            for repo in &config.repositories {
                let sig = repo.sig_level.unwrap_or(config.options.sig_level);
                println!(
                    "   {} ({} server(s), sig: {})",
                    repo.name,
                    repo.server.len(),
                    sig
                );
            }

            // User-added repos
            let user_repos = manager.list().context("failed to list user repositories")?;
            if !user_repos.is_empty() {
                println!();
                println!("   [user-added]");
                for repo in &user_repos {
                    println!("   {} — {}", repo.name, repo.server.join(", "));
                }
            }

            println!();
            let total = config.repositories.len() + user_repos.len();
            println!("   Total: {} repository(ies)", total);
        }
    }

    Ok(())
}

fn cmd_help(args: &cli::HelpArgs) -> Result<()> {
    match args.topic.as_deref() {
        None | Some("") => print_help_overview(),
        Some("commands") => print_help_commands(),
        Some("config") => print_help_config(),
        Some("repos") | Some("repositories") => print_help_repos(),
        Some(cmd) => print_help_command(cmd),
    }
    Ok(())
}

fn print_help_overview() {
    println!(
        r#"xpm — Modern package manager for X Distribution

USAGE:
    xpm <COMMAND> [OPTIONS]
    xpm <ALIAS> [OPTIONS]

QUICK START:
    xpm sync                Synchronize package databases
    xpm install <pkg>       Install a package
    xpm remove <pkg>        Remove a package
    xpm upgrade             Upgrade all packages
    xpm search <query>      Search for packages

TOPICS:
    xpm usage commands      List all available commands
    xpm usage config        Configuration file format
    xpm usage repos         Repository management
    xpm usage <command>     Help for a specific command

GLOBAL FLAGS:
    -c, --config <PATH>     Custom configuration file
    -v, --verbose           Increase verbosity (-v, -vv, -vvv)
    --no-confirm            Skip confirmation prompts
    --root <PATH>           Alternative installation root
    --dbpath <PATH>         Alternative database directory
    --cachedir <PATH>       Alternative cache directory
    --no-color              Disable colored output

PACMAN ALIASES:
    Sy → sync     S → install    R → remove     Su → upgrade
    Q  → query    Ss → search    Si → info      Ql → files

DOCUMENTATION:
    Full CLI reference: docs/CLI.md
    Configuration:      /etc/xpm.conf
    User repos:         /etc/xpm.d/
"#
    );
}

fn print_help_commands() {
    println!(
        r#"xpm — Available Commands

PACKAGE OPERATIONS:
    sync        Synchronize package databases from mirrors
    install     Install one or more packages
    remove      Remove installed packages
    upgrade     Upgrade all installed packages

QUERIES:
    query       Query the local package database
    search      Search for packages in sync databases
    info        Display detailed package information
    files       List files owned by a package

REPOSITORY MANAGEMENT:
    repo add    Add a temporary repository
    repo remove Remove a user-added repository
    repo list   List all active repositories

HELP:
    usage       Display detailed usage information

For detailed help on any command:
    xpm usage <command>
    xpm <command> --help
"#
    );
}

fn print_help_config() {
    println!(
        r#"xpm — Configuration

CONFIGURATION FILE:
    /etc/xpm.conf (TOML format)

GENERAL OPTIONS:
    [options]
    root_dir = "/"                    # Installation root
    db_path = "/var/lib/xpm/"         # Database directory
    cache_dir = "/var/cache/xpm/pkg/" # Package cache
    log_file = "/var/log/xpm.log"     # Log file location
    gpg_dir = "/etc/pacman.d/gnupg/"  # GPG keyring
    sig_level = "optional"            # required | optional | never
    parallel_downloads = 5            # Concurrent downloads
    check_space = true                # Check disk space
    color = true                      # Colored output
    architecture = "x86_64"           # System architecture

PACKAGE LISTS:
    hold_pkg = ["linux"]              # Never upgrade these
    ignore_pkg = ["pkg1", "pkg2"]     # Skip during upgrades
    ignore_group = ["group1"]         # Skip entire groups

REPOSITORY DEFINITION:
    [[repo]]
    name = "core"
    server = [
        "https://mirror.example.com/$repo/os/$arch",
        "https://mirror2.example.com/$repo/os/$arch"
    ]
    sig_level = "required"            # Override global setting

URL VARIABLES:
    $repo   Repository name (e.g., "core", "extra")
    $arch   System architecture (e.g., "x86_64")

FILES:
    /etc/xpm.conf           Main configuration
    /etc/xpm.d/*.toml       User-added repositories
"#
    );
}

fn print_help_repos() {
    println!(
        r#"xpm — Repository Management

PREDEFINED REPOSITORIES:
    Configured in /etc/xpm.conf as [[repo]] sections.
    These are managed by the distribution maintainers.

USER-ADDED REPOSITORIES:
    Stored as individual files in /etc/xpm.d/
    Managed via `xpm repo` commands.

COMMANDS:
    xpm repo list                   List all repositories
    xpm repo add <name> <url>       Add a repository
    xpm repo remove <name>          Remove a repository

EXAMPLES:
    # Add Chaotic-AUR repository
    xpm repo add chaotic-aur https://cdn-mirror.chaotic.cx/$repo/$arch

    # Add a GitHub Pages hosted repo
    xpm repo add my-repo https://user.github.io/my-repo/$arch

    # Add a local file repository
    xpm repo add local file:///srv/packages/$arch

URL VARIABLES:
    $repo   Replaced with the repository name
    $arch   Replaced with system architecture (x86_64, aarch64)

SIGNATURE LEVELS:
    required    Signatures must be present and valid
    optional    Verify if present, allow unsigned (default)
    never       Skip verification completely

After adding a repository, run `xpm sync` to fetch its database.
"#
    );
}

fn print_help_command(cmd: &str) {
    match cmd {
        "sync" | "Sy" => println!(
            r#"xpm sync — Synchronize Package Databases

USAGE:
    xpm sync [OPTIONS]
    xpm Sy [OPTIONS]

DESCRIPTION:
    Downloads the latest package database files from all configured
    repositories. This should be run before installing or upgrading
    packages to ensure you have the latest version information.

OPTIONS:
    -f, --force     Force a full database refresh even if local
                    databases appear to be up to date

EXAMPLES:
    xpm sync            # Normal sync
    xpm sync --force    # Force full refresh
    xpm Sy -f           # Same as above
"#
        ),
        "install" | "S" => println!(
            r#"xpm install — Install Packages

USAGE:
    xpm install <PACKAGES>... [OPTIONS]
    xpm S <PACKAGES>... [OPTIONS]

DESCRIPTION:
    Install one or more packages from the synchronized databases.
    Dependencies are resolved automatically.

ARGUMENTS:
    <PACKAGES>      One or more package names to install

OPTIONS:
    -w, --download-only     Download packages without installing
    --as-deps               Mark as installed as a dependency
    --as-explicit           Mark as explicitly installed
    --no-optional           Skip optional dependencies

EXAMPLES:
    xpm install firefox
    xpm install vim neovim tmux
    xpm S -w linux linux-headers
    xpm install --as-deps libfoo
"#
        ),
        "remove" | "R" => println!(
            r#"xpm remove — Remove Packages

USAGE:
    xpm remove <PACKAGES>... [OPTIONS]
    xpm R <PACKAGES>... [OPTIONS]

DESCRIPTION:
    Remove installed packages from the system.

ARGUMENTS:
    <PACKAGES>      One or more package names to remove

OPTIONS:
    -s, --recursive     Also remove unneeded dependencies
    -d, --no-deps       Skip dependency checking
    -n, --nosave        Remove configuration files (purge)

EXAMPLES:
    xpm remove firefox
    xpm R -s vim           # Remove with unused deps
    xpm remove -n --recursive pkg
"#
        ),
        "upgrade" | "Su" => println!(
            r#"xpm upgrade — System Upgrade

USAGE:
    xpm upgrade [OPTIONS]
    xpm Su [OPTIONS]

DESCRIPTION:
    Upgrade all installed packages to their latest available versions.
    Run `xpm sync` first to get the latest database.

OPTIONS:
    --force             Force reinstall of up-to-date packages
    --ignore <PKG>      Skip specific packages (repeatable)

EXAMPLES:
    xpm upgrade
    xpm Su --ignore linux
    xpm upgrade --ignore pkg1 --ignore pkg2
"#
        ),
        "query" | "Q" => println!(
            r#"xpm query — Query Local Database

USAGE:
    xpm query [FILTER] [OPTIONS]
    xpm Q [FILTER] [OPTIONS]

DESCRIPTION:
    Query the local package database for installed packages.

ARGUMENTS:
    [FILTER]        Optional package name filter

OPTIONS:
    -e, --explicit      List only explicitly installed packages
    -d, --deps          List only packages installed as dependencies
    -t, --orphans       List orphan packages (no longer required)
    -u, --upgrades      List packages with available updates

EXAMPLES:
    xpm query               # List all installed
    xpm Q -e                # Explicit packages only
    xpm query --orphans     # Find orphans
    xpm Q -u                # List upgradeable
"#
        ),
        "search" | "Ss" => println!(
            r#"xpm search — Search Packages

USAGE:
    xpm search <QUERY> [OPTIONS]
    xpm Ss <QUERY> [OPTIONS]

DESCRIPTION:
    Search for packages in the synchronized databases by name,
    description, or provides.

ARGUMENTS:
    <QUERY>         Search term

OPTIONS:
    -l, --local     Search in local database instead of sync

EXAMPLES:
    xpm search firefox
    xpm Ss "text editor"
    xpm search --local vim
"#
        ),
        "info" | "Si" | "Qi" => println!(
            r#"xpm info — Package Information

USAGE:
    xpm info <PACKAGE> [OPTIONS]
    xpm Si <PACKAGE> [OPTIONS]

DESCRIPTION:
    Display detailed information about a package including version,
    description, dependencies, and more.

ARGUMENTS:
    <PACKAGE>       Package name to inspect

OPTIONS:
    -l, --local     Query local database instead of sync

EXAMPLES:
    xpm info linux
    xpm Si firefox
    xpm info --local vim
"#
        ),
        "files" | "Ql" => println!(
            r#"xpm files — List Package Files

USAGE:
    xpm files <PACKAGE>
    xpm Ql <PACKAGE>

DESCRIPTION:
    List all files owned by an installed package.

ARGUMENTS:
    <PACKAGE>       Package name

EXAMPLES:
    xpm files bash
    xpm Ql linux
"#
        ),
        "repo" => println!(
            r#"xpm repo — Repository Management

USAGE:
    xpm repo <ACTION>

ACTIONS:
    list                    List all active repositories
    add <name> <url>        Add a user repository
    remove <name>           Remove a user repository

EXAMPLES:
    xpm repo list
    xpm repo add chaotic-aur https://cdn-mirror.chaotic.cx/$repo/$arch
    xpm repo remove chaotic-aur

See `xpm help repos` for more details on repository configuration.
"#
        ),
        _ => println!(
            "Unknown command or topic: {cmd}\n\n\
             Use `xpm help commands` to see all available commands.\n\
             Use `xpm help` for general help."
        ),
    }
}
