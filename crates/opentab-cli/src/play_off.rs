use crate::cli::PlaybackArgs;
use crate::error::CliError;

pub fn run(_args: &PlaybackArgs) -> Result<(), CliError> {
    Err(CliError::LiveUnavailable)
}
