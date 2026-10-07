use std::path::{Path, PathBuf};

use anyhow::{Result, bail};

use crate::config::{Config, Route};

pub(crate) struct Selection<'a> {
    pub profile: &'a str,
    pub route: Option<&'a Route>,
}

pub(crate) fn select_profile<'a>(
    config: &'a Config,
    path: &Path,
    override_profile: Option<&'a str>,
) -> Result<Selection<'a>> {
    if let Some(profile) = override_profile {
        ensure_profile(config, profile)?;
        return Ok(Selection {
            profile,
            route: None,
        });
    }

    let route = config
        .routes
        .iter()
        .filter(|route| path.starts_with(&route.path))
        .max_by_key(|route| route.path.components().count());
    let profile = route
        .map(|route| route.profile.as_str())
        .unwrap_or(config.default_profile.as_str());

    Ok(Selection { profile, route })
}

pub(crate) fn ensure_profile(config: &Config, profile: &str) -> Result<()> {
    if !config.profiles.contains(profile) {
        bail!("profile '{profile}' does not exist");
    }
    Ok(())
}

pub(crate) fn normalize_existing_directory(path: &Path) -> Result<PathBuf> {
    if !path.is_dir() {
        bail!("{} is not a directory", path.display());
    }
    Ok(path.canonicalize()?)
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use crate::config::{Config, Providers, Route};

    use super::*;

    fn config() -> Config {
        Config {
            version: 1,
            default_profile: "personal".into(),
            providers: Providers::default(),
            profiles: BTreeSet::from(["personal".into(), "work".into(), "client".into()]),
            routes: vec![
                Route {
                    path: "/code/work".into(),
                    profile: "work".into(),
                },
                Route {
                    path: "/code/work/client".into(),
                    profile: "client".into(),
                },
            ],
        }
    }

    #[test]
    fn longest_route_wins() {
        let config = config();
        let selected =
            select_profile(&config, Path::new("/code/work/client/project"), None).unwrap();

        assert_eq!(selected.profile, "client");
    }

    #[test]
    fn directory_boundary_is_preserved() {
        let config = config();
        let selected = select_profile(&config, Path::new("/code/work-old"), None).unwrap();

        assert_eq!(selected.profile, "personal");
    }

    #[test]
    fn explicit_profile_overrides_routes() {
        let config = config();
        let selected = select_profile(&config, Path::new("/code/work"), Some("personal")).unwrap();

        assert_eq!(selected.profile, "personal");
        assert!(selected.route.is_none());
    }
}
