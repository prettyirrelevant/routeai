use std::{fmt::Display, path::Path};

use anyhow::Error;
use console::{Style, style};

pub fn error(error: &Error) {
    eprintln!("{} {}", style("Error:").red().bold(), error);
    for cause in error.chain().skip(1) {
        eprintln!("  {} {cause}", style("Caused by:").dim());
    }
}

pub fn heading(title: &str) {
    println!("{}", style(title).bold());
}

pub fn plain(message: impl Display) {
    println!("{message}");
}

pub fn success(message: impl Display) {
    println!("{} {message}", style("✓").green().bold());
}

pub fn check(message: impl Display) {
    println!("{} {message}", style("✓").green());
}

pub fn problem(message: impl Display) {
    println!("{} {message}", style("✗").red());
}

pub fn warning(message: impl Display) {
    println!("{} {message}", style("!").yellow().bold());
}

pub fn stderr_warning(message: impl Display) {
    eprintln!("{} {message}", style("!").yellow().bold());
}

pub fn field(label: &str, value: impl Display) {
    println!(
        "  {}  {value}",
        Style::new().dim().apply_to(format_args!("{label:<9}"))
    );
}

pub fn item(value: impl Display) {
    println!("  {value}");
}

pub fn next_step(command: impl Display) {
    println!("  {}", style(command).cyan());
}

pub fn blank() {
    println!();
}

pub fn launch(provider: &str, profile: &str) {
    eprintln!(
        "{}  {} {} {}",
        style("routeai").dim(),
        style(provider).bold(),
        style("→").dim(),
        style(profile).cyan()
    );
}

pub fn path_instruction(directory: &Path) {
    heading("Next step");
    println!("Add the shim directory before provider commands on PATH.");
    if cfg!(windows) {
        next_step(format_args!(
            "$env:Path = \"{};$env:Path\"",
            directory.display()
        ));
    } else {
        next_step(format_args!(
            "export PATH=\"{}:$PATH\"",
            directory.display()
        ));
    }
}
