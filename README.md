# routeai

`routeai` selects isolated Claude Code and Codex accounts from your current directory.
It uses the providers' documented state variables instead of moving credential files:

- Claude Code uses `CLAUDE_CONFIG_DIR`.
- Codex uses `CODEX_HOME` with profile-local file credential storage.

The longest matching directory route wins. Every other directory uses your default
profile.

## Install

```sh
cargo install --git https://github.com/prettyirrelevant/routeai
```

## Set up

```sh
routeai init --default personal
routeai profile add work
routeai route add work ~/Developer/Work/Acme
routeai shim install
```

Add the line printed by `routeai shim install` after other `PATH` changes in your shell
profile. Restart your shell, then verify the setup:

```sh
routeai doctor
routeai which
```

Sign each provider into each account:

```sh
routeai login claude --profile personal
routeai login claude --profile work
routeai login codex --profile personal
routeai login codex --profile work
```

After setup, use the original commands normally:

```sh
claude
codex
```

The installed shims call `routeai`, which selects a profile and forwards every argument
to the real provider command.

## Commands

```text
routeai profile add <name>
routeai profile list
routeai profile remove <name>
routeai route add <profile> <directory>
routeai route remove <directory>
routeai route list
routeai default <profile>
routeai which [directory]
routeai status [--profile <name>]
routeai doctor
```

Use one profile without changing routes:

```sh
routeai run claude --profile work -- --continue
ROUTEAI_PROFILE=work codex
```

## Configuration

On macOS and Linux, the configuration lives at `~/.config/routeai/config.toml`.
Account state lives under `~/.local/share/routeai/profiles/`. Windows uses its standard
user configuration and local data directories.

Set `ROUTEAI_CONFIG` to use another configuration file. Set `ROUTEAI_HOME` to use
another state directory.

`routeai init` detects the provider commands from `PATH`. Edit their paths in
`config.toml` if either provider moves later.

## Security

`routeai` separates application state. It does not create an operating-system sandbox.
Every provider process retains your normal filesystem permissions.

Codex profile credentials live in each profile's `auth.json`. Treat the routeai state
directory as sensitive and never commit it.

Claude Code currently has an open report about user-level `CLAUDE.md` leakage across
custom configuration directories. Do not treat profile separation as a confidentiality
control.
