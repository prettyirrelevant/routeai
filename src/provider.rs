use std::{
    env,
    ffi::{OsStr, OsString},
    fs,
    path::{Path, PathBuf},
    process::{Command, ExitStatus},
};

use anyhow::{Context, Result, bail};
use clap::ValueEnum;

use crate::config::{Config, profile_home};

#[derive(Debug, Clone, Copy, ValueEnum)]
pub enum Provider {
    Claude,
    Codex,
}

impl Provider {
    pub fn name(self) -> &'static str {
        match self {
            Self::Claude => "claude",
            Self::Codex => "codex",
        }
    }

    pub fn env_name(self) -> &'static str {
        match self {
            Self::Claude => "CLAUDE_CONFIG_DIR",
            Self::Codex => "CODEX_HOME",
        }
    }

    pub fn command(self, config: &Config) -> Option<&Path> {
        match self {
            Self::Claude => config.providers.claude.command.as_deref(),
            Self::Codex => config.providers.codex.command.as_deref(),
        }
    }

    pub fn login_args(self) -> &'static [&'static str] {
        match self {
            Self::Claude => &["auth", "login"],
            Self::Codex => &["login"],
        }
    }

    pub fn status_args(self) -> &'static [&'static str] {
        match self {
            Self::Claude => &["auth", "status", "--text"],
            Self::Codex => &["login", "status"],
        }
    }
}

pub fn configure_command(command: &mut Command, provider: Provider, profile: &str) -> Result<()> {
    let home = profile_home(profile, provider.name())?;
    fs::create_dir_all(&home)?;
    command.env(provider.env_name(), home);
    Ok(())
}

pub fn wait(
    config: &Config,
    provider: Provider,
    profile: &str,
    args: impl IntoIterator<Item = OsString>,
) -> Result<ExitStatus> {
    let (mut command, executable) = build_command(config, provider, profile, args)?;
    command
        .status()
        .with_context(|| format!("could not run {}", executable.display()))
}

pub fn execute(
    config: &Config,
    provider: Provider,
    profile: &str,
    args: impl IntoIterator<Item = OsString>,
) -> Result<u8> {
    let (mut command, executable) = build_command(config, provider, profile, args)?;

    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        let error = command.exec();
        Err(error).with_context(|| format!("could not run {}", executable.display()))
    }

    #[cfg(windows)]
    {
        let status = command
            .status()
            .with_context(|| format!("could not run {}", executable.display()))?;
        Ok(status.code().unwrap_or(1).clamp(0, u8::MAX as i32) as u8)
    }
}

fn build_command<'a>(
    config: &'a Config,
    provider: Provider,
    profile: &str,
    args: impl IntoIterator<Item = OsString>,
) -> Result<(Command, &'a Path)> {
    let executable = provider.command(config).with_context(|| {
        format!(
            "{} command is not configured; rerun 'routeai init' or edit the config",
            provider.name()
        )
    })?;
    let mut command = Command::new(executable);
    command.args(args);
    configure_command(&mut command, provider, profile)?;
    Ok((command, executable))
}

pub fn discover(name: &str) -> Option<PathBuf> {
    let current = env::current_exe()
        .ok()
        .and_then(|path| path.canonicalize().ok());
    let paths = env::var_os("PATH")?;
    for directory in env::split_paths(&paths) {
        #[cfg(unix)]
        let candidates = [directory.join(name)];
        #[cfg(windows)]
        let candidates = [
            directory.join(format!("{name}.exe")),
            directory.join(format!("{name}.cmd")),
            directory.join(format!("{name}.bat")),
        ];
        for candidate in candidates {
            if !candidate.is_file() || crate::shims::is_managed(&candidate, current.as_deref()) {
                continue;
            }
            let canonical = candidate.canonicalize().ok();
            if canonical.is_some() && canonical != current {
                if candidate.is_absolute() {
                    return Some(candidate);
                }
                let parent = candidate.parent()?.canonicalize().ok()?;
                return Some(parent.join(candidate.file_name()?));
            }
        }
    }
    None
}

pub fn provider_from_invocation(value: &OsStr) -> Option<Provider> {
    let stem = Path::new(value).file_stem()?.to_string_lossy();
    match stem.as_ref() {
        "claude" => Some(Provider::Claude),
        "codex" => Some(Provider::Codex),
        _ => None,
    }
}

pub fn ensure_distinct_command(config: &Config, provider: Provider) -> Result<()> {
    let Some(command) = provider.command(config) else {
        return Ok(());
    };
    let current = env::current_exe()?.canonicalize()?;
    if crate::shims::is_managed(command, Some(&current)) {
        bail!(
            "{} command resolves to routeai and would recurse",
            provider.name()
        );
    }
    Ok(())
}
