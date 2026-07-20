use crate::cli::args::ServeArgs;
use crate::error::OpenPageResult;

pub fn run(args: ServeArgs) -> OpenPageResult<()> {
    openpage::daemon::run_tcp(args.port.unwrap_or(0), &args.session)
}
