use std::process::ExitCode;

mod app;
mod config;
mod provider;
mod routing;
mod shims;
mod ui;

fn main() -> ExitCode {
    match app::run() {
        Ok(code) => ExitCode::from(code),
        Err(error) => {
            ui::error(&error);
            ExitCode::FAILURE
        }
    }
}
