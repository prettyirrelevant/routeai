use std::{
    env,
    ffi::OsString,
    path::{Path, PathBuf},
};

use anyhow::{Context, Result, bail};
use clap::{Parser, Subcommand};

use crate::{
    config::{
        self, Config, Providers, Route, initialize_profile, route_index, validate_profile_name,
    },
    provider::{self, Provider},
    routing::{ensure_profile, normalize_existing_directory, select_profile},
    shims, ui,
};

#[derive(Parser)]
#[command(version, about)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Create the configuration and first profile.
    Init {
        #[arg(long, default_value = "personal")]
        default: String,
    },
    /// Manage account profiles.
    Profile {
        #[command(subcommand)]
        command: ProfileCommand,
    },
    /// Manage directory routes.
    Route {
        #[command(subcommand)]
        command: RouteCommand,
    },
    /// Change the fallback profile.
    Default { profile: String },
    /// Show the profile selected for a path.
    Which {
        #[arg(default_value = ".")]
        path: PathBuf,
        #[arg(long)]
        verbose: bool,
    },
    /// Sign a profile in to a provider.
    Login {
        provider: Provider,
        #[arg(long)]
        profile: String,
    },
    /// Show provider login status for one profile.
    Status {
        #[arg(long)]
        profile: Option<String>,
    },
    /// Install or remove transparent claude and codex shims.
    Shim {
        #[command(subcommand)]
        command: ShimCommand,
    },
    /// Check configuration, commands, routes, and shims.
    Doctor,
}

#[derive(Subcommand)]
enum ProfileCommand {
    /// Create an isolated account profile.
    Add { name: String },
    /// List configured profiles.
    List,
    /// Remove a profile from the configuration.
    Remove {
        name: String,
        #[arg(long)]
        delete_state: bool,
    },
}

#[derive(Subcommand)]
enum RouteCommand {
    /// Route a directory to a profile.
    Add { profile: String, path: PathBuf },
    /// Remove a directory route.
    Remove { path: PathBuf },
    /// List directory routes.
    List,
}

#[derive(Subcommand)]
enum ShimCommand {
    /// Install transparent claude and codex commands.
    Install,
    /// Remove transparent claude and codex commands.
    Uninstall,
}

pub(crate) fn run() -> Result<u8> {
    if let Some(provider) = env::args_os()
        .next()
        .as_deref()
        .and_then(provider::provider_from_invocation)
    {
        return launch(provider, None, env::args_os().skip(1));
    }

    let mut args = env::args_os().skip(1);
    if args.next().is_some_and(|arg| arg == "__run") {
        let provider = args
            .next()
            .as_deref()
            .and_then(provider::provider_from_invocation)
            .context("internal provider is missing")?;
        return launch(provider, None, args);
    }

    match Cli::parse().command {
        Commands::Init { default } => initialize(default)?,
        Commands::Profile { command } => profile(command)?,
        Commands::Route { command } => route(command)?,
        Commands::Default { profile } => set_default(profile)?,
        Commands::Which { path, verbose } => show_selection(path, verbose)?,
        Commands::Login { provider, profile } => {
            return run_for_profile(provider, &profile, provider.login_args());
        }
        Commands::Status { profile } => status(profile)?,
        Commands::Shim { command } => shim(command)?,
        Commands::Doctor => doctor()?,
    }
    Ok(0)
}

fn initialize(default: String) -> Result<()> {
    validate_profile_name(&default)?;
    let path = config::config_path()?;
    if path.exists() {
        bail!("{} already exists", path.display());
    }
    let providers = Providers {
        claude: config::ProviderConfig {
            command: provider::discover("claude"),
        },
        codex: config::ProviderConfig {
            command: provider::discover("codex"),
        },
    };
    let config = Config::new(default.clone(), providers);
    initialize_profile(&default)?;
    config::save(&config)?;
    let shim_directory = shims::install(&config)?;
    ui::success("Initialized routeai");
    ui::field("Config", path.display());
    ui::field("Default", &default);
    print_commands(&config);
    ui::blank();
    show_installed_shims(&shim_directory);
    ui::blank();
    ui::heading("Next steps");
    ui::next_step("routeai profile add work");
    ui::next_step("routeai route add work /path/to/work");
    Ok(())
}

fn profile(command: ProfileCommand) -> Result<()> {
    let mut config = config::load()?;
    match command {
        ProfileCommand::Add { name } => {
            validate_profile_name(&name)?;
            if config.profiles.contains(&name) {
                bail!("profile '{name}' already exists");
            }
            initialize_profile(&name)?;
            config.profiles.insert(name.clone());
            config::save(&config)?;
            ui::success(format_args!("Added profile '{name}'"));
            ui::field(
                "State",
                config::state_root()?.join("profiles").join(&name).display(),
            );
        }
        ProfileCommand::List => {
            ui::heading("Profiles");
            for name in &config.profiles {
                if name == &config.default_profile {
                    ui::field(name, "default");
                } else {
                    ui::field(name, "");
                }
            }
        }
        ProfileCommand::Remove { name, delete_state } => {
            ensure_profile(&config, &name)?;
            if name == config.default_profile {
                bail!("choose another default profile before removing '{name}'");
            }
            if config.routes.iter().any(|route| route.profile == name) {
                bail!("remove routes for profile '{name}' first");
            }
            config.profiles.remove(&name);
            config::save(&config)?;
            if delete_state {
                let path = config::state_root()?.join("profiles").join(&name);
                if path.exists() {
                    std::fs::remove_dir_all(&path)?;
                }
            }
            ui::success(format_args!("Removed profile '{name}'"));
            if !delete_state {
                ui::warning("Profile state remains on disk");
                ui::field(
                    "State",
                    config::state_root()?.join("profiles").join(&name).display(),
                );
            }
        }
    }
    Ok(())
}

fn route(command: RouteCommand) -> Result<()> {
    let mut config = config::load()?;
    match command {
        RouteCommand::Add { profile, path } => {
            ensure_profile(&config, &profile)?;
            let path = normalize_existing_directory(&path)?;
            if let Some(index) = route_index(&config, &path) {
                config.routes[index].profile = profile.clone();
                ui::success("Updated route");
            } else {
                config.routes.push(Route {
                    path: path.clone(),
                    profile: profile.clone(),
                });
                ui::success("Added route");
            }
            config::save(&config)?;
            ui::field("Directory", path.display());
            ui::field("Profile", profile);
        }
        RouteCommand::Remove { path } => {
            let path = if path.exists() {
                normalize_existing_directory(&path)?
            } else {
                std::path::absolute(&path)?
            };
            let index = route_index(&config, &path)
                .with_context(|| format!("no route exists for {}", path.display()))?;
            config.routes.remove(index);
            config::save(&config)?;
            ui::success("Removed route");
            ui::field("Directory", path.display());
        }
        RouteCommand::List => {
            ui::heading("Routes");
            let mut routes = config.routes.iter().collect::<Vec<_>>();
            routes.sort_by(|left, right| left.path.cmp(&right.path));
            for route in routes {
                ui::field(&route.profile, route.path.display());
            }
            ui::field("default", &config.default_profile);
        }
    }
    Ok(())
}

fn set_default(profile: String) -> Result<()> {
    let mut config = config::load()?;
    ensure_profile(&config, &profile)?;
    config.default_profile = profile.clone();
    config::save(&config)?;
    ui::success(format_args!("Set default profile to '{profile}'"));
    Ok(())
}

fn show_selection(path: PathBuf, verbose: bool) -> Result<()> {
    let config = config::load()?;
    let path = normalize_existing_directory(&path)?;
    let profile_override = env_profile();
    let selection = select_profile(&config, &path, profile_override.as_deref())?;
    ui::plain(selection.profile);
    if verbose {
        if let Some(route) = selection.route {
            ui::field("Route", route.path.display());
        } else if profile_override.is_some() {
            ui::field("Source", "ROUTEAI_PROFILE");
        } else {
            ui::field("Source", "default");
        }
    }
    Ok(())
}

fn launch(
    provider: Provider,
    profile: Option<String>,
    args: impl IntoIterator<Item = OsString>,
) -> Result<u8> {
    let config = config::load()?;
    provider::ensure_distinct_command(&config, provider)?;
    let cwd = env::current_dir()?.canonicalize()?;
    let profile = profile.or_else(env_profile);
    let selection = select_profile(&config, &cwd, profile.as_deref())?;
    if atty_stderr() {
        ui::launch(provider.name(), selection.profile);
        warn_about_override(provider);
    }
    provider::execute(&config, provider, selection.profile, args)
}

fn run_for_profile(provider: Provider, profile: &str, args: &[&str]) -> Result<u8> {
    let config = config::load()?;
    ensure_profile(&config, profile)?;
    provider::ensure_distinct_command(&config, provider)?;
    ui::launch(provider.name(), profile);
    provider::execute(&config, provider, profile, args.iter().map(OsString::from))
}

fn status(profile: Option<String>) -> Result<()> {
    let config = config::load()?;
    let selected = match profile {
        Some(profile) => {
            ensure_profile(&config, &profile)?;
            profile
        }
        None => {
            let cwd = env::current_dir()?.canonicalize()?;
            let profile_override = env_profile();
            select_profile(&config, &cwd, profile_override.as_deref())?
                .profile
                .to_owned()
        }
    };
    ui::heading("Account status");
    ui::field("Profile", &selected);
    ui::blank();
    for provider in [Provider::Claude, Provider::Codex] {
        ui::heading(provider.name());
        if provider.command(&config).is_none() {
            ui::warning("Command is not configured");
            continue;
        }
        let status = provider::wait(
            &config,
            provider,
            &selected,
            provider.status_args().iter().map(OsString::from),
        )?;
        if !status.success() {
            ui::warning("This profile is not logged in");
        }
        ui::blank();
    }
    Ok(())
}

fn shim(command: ShimCommand) -> Result<()> {
    match command {
        ShimCommand::Install => {
            let config = config::load()?;
            let directory = shims::install(&config)?;
            show_installed_shims(&directory);
        }
        ShimCommand::Uninstall => {
            let directory = shims::uninstall()?;
            ui::success("Removed claude and codex shims");
            ui::field("Directory", directory.display());
        }
    }
    Ok(())
}

fn show_installed_shims(directory: &Path) {
    ui::success("Installed claude and codex shims");
    ui::field("Directory", directory.display());
    if shims::on_path(directory) {
        ui::field("Status", "ready in new shells");
    } else {
        ui::blank();
        ui::path_instruction(directory);
    }
}

fn doctor() -> Result<()> {
    let config = config::load()?;
    let mut problems = 0;
    let mut checks = 1;
    ui::heading("routeai doctor");
    ui::check(format_args!(
        "Configuration  {}",
        config::config_path()?.display()
    ));
    for provider in [Provider::Claude, Provider::Codex] {
        checks += 1;
        match provider.command(&config) {
            Some(path) if path.is_file() => {
                ui::check(format_args!("{:<14} {}", provider.name(), path.display()));
            }
            Some(path) => {
                problems += 1;
                ui::problem(format_args!(
                    "{:<14} {} does not exist",
                    provider.name(),
                    path.display()
                ));
            }
            None => {
                problems += 1;
                ui::problem(format_args!(
                    "{:<14} command is not configured",
                    provider.name()
                ));
            }
        }
    }
    for route in &config.routes {
        checks += 1;
        if route.path.is_dir() {
            ui::check(format_args!(
                "Route          {} → {}",
                route.path.display(),
                route.profile
            ));
        } else {
            problems += 1;
            ui::problem(format_args!(
                "Route          {} does not exist",
                route.path.display()
            ));
        }
    }
    let shim_dir = shims::directory()?;
    let mut shim_problem = false;
    if shims::installed()? {
        ui::check("Shims          claude and codex are installed");
    } else {
        problems += 1;
        shim_problem = true;
        ui::problem("Shims          claude and codex are not installed");
    }
    checks += 1;
    let path_precedes_commands = shims::precedes_provider_commands(&shim_dir, &config);
    if path_precedes_commands {
        ui::check("PATH           shims precede provider commands");
    } else {
        problems += 1;
        shim_problem = true;
        ui::problem("PATH           shim directory is missing or too late");
    }
    checks += 1;
    if env::var_os("ANTHROPIC_API_KEY").is_some() {
        ui::warning("ANTHROPIC_API_KEY overrides Claude subscription routing");
    }
    if env::var_os("CODEX_API_KEY").is_some() {
        ui::warning("CODEX_API_KEY overrides authentication in some Codex commands");
    }
    if problems > 0 {
        if shim_problem {
            ui::blank();
            ui::path_instruction(&shim_dir);
        }
        bail!("doctor found {problems} problem(s)");
    }
    ui::blank();
    ui::success(format_args!("All {checks} checks passed"));
    Ok(())
}

fn print_commands(config: &Config) {
    for provider in [Provider::Claude, Provider::Codex] {
        match provider.command(config) {
            Some(path) => ui::field(provider.name(), path.display()),
            None => ui::field(provider.name(), "not found"),
        }
    }
}

fn warn_about_override(provider: Provider) {
    let variable = match provider {
        Provider::Claude if env::var_os("ANTHROPIC_API_KEY").is_some() => Some("ANTHROPIC_API_KEY"),
        Provider::Codex if env::var_os("CODEX_API_KEY").is_some() => Some("CODEX_API_KEY"),
        _ => None,
    };
    if let Some(variable) = variable {
        ui::stderr_warning(format_args!("{variable} can override this profile"));
    }
}

fn env_profile() -> Option<String> {
    env::var("ROUTEAI_PROFILE")
        .ok()
        .filter(|value| !value.is_empty())
}

fn atty_stderr() -> bool {
    use std::io::IsTerminal;
    std::io::stderr().is_terminal()
}
