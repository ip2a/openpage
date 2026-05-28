pub mod args;
pub mod protocol;

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

    let result = match cli.command {
        Command::Serve(args) => serve::run(args),
        command => oneshot::run(command),
    };

    match result {
        Ok(()) => Ok(0),
        Err(err) => {
            println!(
                "{}",
                serde_json::to_string(&protocol::simple_error("openpage", err.to_string()))
                    .map_err(|err| OpenPageError::Serialization(err.to_string()))?
            );
            Ok(1)
        }
    }
}
