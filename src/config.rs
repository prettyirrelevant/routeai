use std::{
    collections::BTreeSet,
    env, fs,
    path::{Path, PathBuf},
};

use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Config {
    pub version: u8,
    pub default_profile: String,
    #[serde(default)]
    pub providers: Providers,
    #[serde(default)]
    pub profiles: BTreeSet<String>,
    #[serde(default)]
    pub routes: Vec<Route>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Providers {
    #[serde(default)]
    pub claude: ProviderConfig,
    #[serde(default)]
    pub codex: ProviderConfig,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProviderConfig {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub command: Option<PathBuf>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Route {
    pub path: PathBuf,
    pub profile: String,
}

impl Config {
    pub fn new(default_profile: String, providers: Providers) -> Self {
        Self {
            version: 1,
            default_profile: default_profile.clone(),
            providers,
            profiles: BTreeSet::from([default_profile]),
            routes: Vec::new(),
        }
    }

    pub fn validate(&self) -> Result<()> {
        if self.version != 1 {
            bail!("unsupported config version {}", self.version);
        }
        for profile in &self.profiles {
            validate_profile_name(profile)?;
        }
        if !self.profiles.contains(&self.default_profile) {
            bail!("default profile '{}' does not exist", self.default_profile);
        }
        for (name, provider) in [
            ("claude", &self.providers.claude),
            ("codex", &self.providers.codex),
        ] {
            if provider
                .command
                .as_ref()
                .is_some_and(|command| !command.is_absolute())
            {
                bail!("{name} command must be an absolute path");
            }
        }
        for route in &self.routes {
            if !self.profiles.contains(&route.profile) {
                bail!(
                    "route {} uses missing profile '{}'",
                    route.path.display(),
                    route.profile
                );
            }
            if !route.path.is_absolute() {
                bail!("route {} is not absolute", route.path.display());
            }
        }
        for (index, route) in self.routes.iter().enumerate() {
            if self.routes[index + 1..]
                .iter()
                .any(|candidate| candidate.path == route.path)
            {
                bail!("duplicate route for {}", route.path.display());
            }
        }
        Ok(())
    }
}

pub fn config_path() -> Result<PathBuf> {
    if let Some(path) = env::var_os("ROUTEAI_CONFIG") {
        return Ok(std::path::absolute(path)?);
    }
    Ok(config_root()?.join("config.toml"))
}

pub fn config_root() -> Result<PathBuf> {
    if let Some(path) = env::var_os("XDG_CONFIG_HOME") {
        return Ok(PathBuf::from(path).join("routeai"));
    }
    if cfg!(windows) {
        return Ok(dirs::config_dir()
            .context("could not find the configuration directory")?
            .join("routeai"));
    }
    Ok(home_dir()?.join(".config/routeai"))
}

pub fn state_root() -> Result<PathBuf> {
    if let Some(path) = env::var_os("ROUTEAI_HOME") {
        return Ok(std::path::absolute(path)?);
    }
    if let Some(path) = env::var_os("XDG_DATA_HOME") {
        return Ok(PathBuf::from(path).join("routeai"));
    }
    if cfg!(windows) {
        return Ok(dirs::data_local_dir()
            .context("could not find the local data directory")?
            .join("routeai"));
    }
    Ok(home_dir()?.join(".local/share/routeai"))
}

fn home_dir() -> Result<PathBuf> {
    dirs::home_dir().context("could not find the home directory")
}

pub fn load() -> Result<Config> {
    let path = config_path()?;
    let text = fs::read_to_string(&path)
        .with_context(|| format!("could not read {}; run 'routeai init'", path.display()))?;
    let config: Config =
        toml::from_str(&text).with_context(|| format!("could not parse {}", path.display()))?;
    config.validate()?;
    Ok(config)
}

pub fn save(config: &Config) -> Result<()> {
    config.validate()?;
    let path = config_path()?;
    let parent = path.parent().context("config path has no parent")?;
    fs::create_dir_all(parent)?;
    let text = toml::to_string_pretty(config)?;
    let temporary = path.with_extension(format!("toml.{}.tmp", std::process::id()));
    fs::write(&temporary, text)?;
    if cfg!(windows) && path.exists() {
        fs::remove_file(&path)?;
    }
    fs::rename(&temporary, &path)?;
    Ok(())
}

/// Global instruction files that each new profile links from the provider's default home.
const GLOBAL_INSTRUCTIONS: [(&str, &str, &str); 2] = [
    ("claude", ".claude", "CLAUDE.md"),
    ("codex", ".codex", "AGENTS.md"),
];

/// Creates the profile state and returns the global instruction files it linked.
pub fn initialize_profile(name: &str) -> Result<Vec<PathBuf>> {
    validate_profile_name(name)?;
    let root = state_root()?.join("profiles").join(name);
    fs::create_dir_all(root.join("claude"))?;
    let codex = root.join("codex");
    fs::create_dir_all(&codex)?;
    restrict_directory(&root)?;
    restrict_directory(&root.join("claude"))?;
    restrict_directory(&codex)?;
    let codex_config = codex.join("config.toml");
    if !codex_config.exists() {
        fs::write(
            codex_config,
            "# routeai keeps this profile's credentials under CODEX_HOME.\ncli_auth_credentials_store = \"file\"\n",
        )?;
    }
    link_global_instructions(&root)
}

fn link_global_instructions(root: &Path) -> Result<Vec<PathBuf>> {
    let home = home_dir()?;
    let mut linked = Vec::new();
    for (provider, directory, file) in GLOBAL_INSTRUCTIONS {
        let source = home.join(directory).join(file);
        let target = root.join(provider).join(file);
        if !source.is_file() || target.symlink_metadata().is_ok() {
            continue;
        }
        link_file(&source, &target)
            .with_context(|| format!("could not link {}", source.display()))?;
        linked.push(target);
    }
    Ok(linked)
}

#[cfg(unix)]
fn link_file(source: &Path, target: &Path) -> std::io::Result<()> {
    std::os::unix::fs::symlink(source, target)
}

// Windows file symlinks need extra privileges, so profiles there keep a copy.
#[cfg(windows)]
fn link_file(source: &Path, target: &Path) -> std::io::Result<()> {
    fs::copy(source, target).map(|_| ())
}

#[cfg(unix)]
fn restrict_directory(path: &Path) -> Result<()> {
    use std::os::unix::fs::PermissionsExt;

    fs::set_permissions(path, fs::Permissions::from_mode(0o700))?;
    Ok(())
}

#[cfg(windows)]
fn restrict_directory(_: &Path) -> Result<()> {
    Ok(())
}

pub fn profile_home(name: &str, provider: &str) -> Result<PathBuf> {
    Ok(state_root()?.join("profiles").join(name).join(provider))
}

pub fn validate_profile_name(name: &str) -> Result<()> {
    let valid = !name.is_empty()
        && name
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-' || byte == b'_');
    if !valid {
        bail!("profile names may contain only letters, numbers, '-' and '_'");
    }
    Ok(())
}

pub fn route_index(config: &Config, path: &Path) -> Option<usize> {
    config.routes.iter().position(|route| route.path == path)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn profile_names_reject_path_characters() {
        assert!(validate_profile_name("work/client").is_err());
        assert!(validate_profile_name("work.client").is_err());
        assert!(validate_profile_name("work-client_2").is_ok());
    }

    #[test]
    fn config_rejects_duplicate_routes() {
        let mut config = Config::new("personal".into(), Providers::default());
        config.routes = vec![
            Route {
                path: "/code".into(),
                profile: "personal".into(),
            },
            Route {
                path: "/code".into(),
                profile: "personal".into(),
            },
        ];

        assert!(config.validate().is_err());
    }

    #[test]
    fn config_rejects_profile_path_traversal() {
        let mut config = Config::new("personal".into(), Providers::default());
        config.profiles.insert("../../outside".into());

        assert!(config.validate().is_err());
    }
}
