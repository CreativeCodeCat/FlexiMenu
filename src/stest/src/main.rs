use clap::{CommandFactory, FromArgMatches};
use std::io;
use std::io::{stdin, stdout};
use std::process::ExitCode;
use stest::config::Config;
use stest::App;

/// The stest program filters a list of files by their properties, in a way that is analogous to
/// bash's builtin test, except that files that pass all tests are printed to stdout.
fn main() -> ExitCode {
    // clap needs a 'static version string; this is read once at startup, so leaking it is fine
    let version: &'static str = Box::leak(
        std::env::var("VERSION")
            .unwrap_or_else(|_| "unknown".to_string())
            .into_boxed_str(),
    );
    let matches = Config::command().version(version).get_matches();
    let config = Config::from_arg_matches(&matches).unwrap_or_else(|e| e.exit());
    let mut stdin = stdin().lock();
    let mut stdout = stdout().lock();
    let result = App::new(config).run(&mut stdin, &mut stdout);
    match result {
        Ok(true) => ExitCode::SUCCESS,
        Ok(false) => ExitCode::FAILURE,
        Err(error) => match error {
            io::Error { .. } => {
                print!("{}", error);
                ExitCode::FAILURE
            }
        },
    }
}
