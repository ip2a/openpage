pub mod args;
pub mod connection;
pub mod protocol;

mod doctor;
mod oneshot;
mod serve;

use clap::Parser;
use clap::error::ErrorKind;

use crate::cli::args::{Cli, Command};
use crate::error::{OpenPageError, OpenPageResult};

pub fn run() -> OpenPageResult<i32> {
    run_from_args(std::env::args_os())
}

pub fn run_from_args<I, T>(args: I) -> OpenPageResult<i32>
where
    I: IntoIterator<Item = T>,
    T: Into<std::ffi::OsString> + Clone,
{
    let cli = match Cli::try_parse_from(args) {
        Ok(cli) => cli,
        Err(err)
            if matches!(
                err.kind(),
                ErrorKind::DisplayHelp | ErrorKind::DisplayVersion
            ) =>
        {
            err.print()
                .map_err(|err| OpenPageError::Io(err.to_string()))?;
            return Ok(0);
        }
        Err(err) => {
            err.print()
                .map_err(|err| OpenPageError::Io(err.to_string()))?;
            return Ok(err.exit_code());
        }
    };

    match cli.command {
        Command::Serve(args) => match serve::run(args) {
            Ok(()) => Ok(0),
            Err(err) => {
                println!(
                    "{}",
                    protocol::format_output_json(&protocol::simple_openpage_error(&err))
                        .map_err(|err| OpenPageError::Serialization(err.to_string()))?
                );
                Ok(1)
            }
        },
        Command::Doctor(args) => match doctor::run(args) {
            Ok(code) => Ok(code),
            Err(err) => {
                println!(
                    "{}",
                    protocol::format_output_json(&protocol::simple_openpage_error(&err))
                        .map_err(|err| OpenPageError::Serialization(err.to_string()))?
                );
                Ok(1)
            }
        },
        command => match oneshot::run(command) {
            Ok(code) => Ok(code),
            Err(err) => {
                println!(
                    "{}",
                    protocol::format_output_json(&protocol::simple_openpage_error(&err))
                        .map_err(|err| OpenPageError::Serialization(err.to_string()))?
                );
                Ok(1)
            }
        },
    }
}
