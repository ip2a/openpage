use std::io::{BufReader, stdin, stdout};

use crate::cli::args::McpArgs;
use crate::error::OpenPageResult;

pub fn run(args: McpArgs) -> OpenPageResult<()> {
    let input = BufReader::new(stdin().lock());
    openpage::mcp::serve_stdio(&args.session, input, stdout().lock())
}
