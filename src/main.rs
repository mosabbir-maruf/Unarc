//! Unarc binary entry point.

use std::process::ExitCode;

fn main() -> ExitCode {
    match unarc::run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(err) => {
            // Note: error output has already been rendered by the CLI formatter
            ExitCode::from(err.exit_code() as u8)
        }
    }
}
