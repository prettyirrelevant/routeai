#![cfg(unix)]

use std::{fs, os::unix::fs::PermissionsExt, path::Path};

use assert_cmd::cargo::cargo_bin_cmd;
use predicates::prelude::*;
use tempfile::TempDir;

struct TestHome {
    temp: TempDir,
}

impl TestHome {
    fn new() -> Self {
        let temp = TempDir::new().unwrap();
        let bin = temp.path().join("bin");
        fs::create_dir(&bin).unwrap();
        executable(
            &bin.join("claude"),
            "#!/bin/sh\nprintf '%s\\n' \"$CLAUDE_CONFIG_DIR\"\nprintf '%s\\n' \"$@\"\n",
        );
        executable(
            &bin.join("codex"),
            "#!/bin/sh\n[ \"$1\" = fail ] && exit 42\nprintf '%s\\n' \"$CODEX_HOME\"\n",
        );
        Self { temp }
    }

    fn root(&self) -> &Path {
        self.temp.path()
    }

    fn command(&self) -> assert_cmd::Command {
        let mut command = cargo_bin_cmd!("routeai");
        command
            .env("ROUTEAI_CONFIG", self.root().join("config.toml"))
            .env("ROUTEAI_HOME", self.root().join("state"))
            .env("PATH", self.root().join("bin"));
        command
    }

    fn provider(&self, name: &str) -> assert_cmd::Command {
        let mut command = assert_cmd::Command::new(self.root().join("state/bin").join(name));
        command
            .env("ROUTEAI_CONFIG", self.root().join("config.toml"))
            .env("ROUTEAI_HOME", self.root().join("state"))
            .env("PATH", self.root().join("bin"));
        command
    }
}

fn executable(path: &Path, content: &str) {
    fs::write(path, content).unwrap();
    let mut permissions = fs::metadata(path).unwrap().permissions();
    permissions.set_mode(0o755);
    fs::set_permissions(path, permissions).unwrap();
}

#[test]
fn routes_provider_state_by_directory() {
    let home = TestHome::new();
    home.command().args(["init"]).assert().success();
    home.command()
        .args(["profile", "add", "work"])
        .assert()
        .success();

    let work = home.root().join("work/project");
    fs::create_dir_all(&work).unwrap();
    home.command()
        .arg("route")
        .arg("add")
        .arg("work")
        .arg(home.root().join("work"))
        .assert()
        .success();

    home.command()
        .current_dir(&work)
        .args(["which"])
        .assert()
        .success()
        .stdout(predicate::str::starts_with("work\n"));

    home.provider("claude")
        .current_dir(&work)
        .arg("--continue")
        .assert()
        .success()
        .stdout(
            predicate::str::contains("state/profiles/work/claude")
                .and(predicate::str::contains("--continue")),
        );

    home.provider("codex")
        .current_dir(home.root())
        .assert()
        .success()
        .stdout(predicate::str::contains("state/profiles/personal/codex"));
}

#[test]
fn preserves_provider_exit_code() {
    let home = TestHome::new();
    home.command().args(["init"]).assert().success();

    home.provider("codex").arg("fail").assert().code(42);
}

#[test]
fn installs_both_transparent_shims() {
    let home = TestHome::new();
    home.command().args(["init"]).assert().success();

    let bin = home.root().join("state/bin");
    assert!(bin.join("claude").exists());
    assert!(bin.join("codex").exists());
}

#[test]
fn profile_override_beats_directory_route() {
    let home = TestHome::new();
    home.command().args(["init"]).assert().success();
    home.command()
        .args(["profile", "add", "work"])
        .assert()
        .success();

    home.provider("codex")
        .env("ROUTEAI_PROFILE", "work")
        .assert()
        .success()
        .stdout(predicate::str::contains("state/profiles/work/codex"));
}
