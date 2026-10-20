use thiserror::Error;

use crate::BankError;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum InstallStage {
    Downloading,
    Verifying,
    Unpacking,
    Registering,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct InstallProgress {
    pub stage: InstallStage,
    pub done: u64,
    pub total: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum InstallError {
    #[error("the catalogue has no sound bank named {id}")]
    Unknown { id: String },
    #[error("{url} is not a web address OpenTab can download from")]
    Address { url: String },
    #[error(
        "OpenTab downloads sound banks over secure (https) connections only, so it did not download {url}"
    )]
    Insecure { url: String },
    #[error(
        "OpenTab could not reach the download server. Check the internet connection and try again. ({message})"
    )]
    Offline { message: String },
    #[error("the download server answered with error {status}")]
    Http { status: u16 },
    #[error(
        "the download is larger than the {expected} bytes the catalogue lists, so it was not installed"
    )]
    TooLarge { expected: u64 },
    #[error("the download stopped after {received} of {expected} bytes. Try again.")]
    Truncated { expected: u64, received: u64 },
    #[error(
        "the downloaded file does not match the checksum in OpenTab's catalogue, so it was not installed. The file on the server may have changed; a later OpenTab release will update this entry."
    )]
    HashMismatch { expected: String, actual: String },
    #[error("the download failed: {message}")]
    Network { message: String },
    #[error("the download was cancelled, and nothing was installed")]
    Cancelled,
    #[error("{0}")]
    Bank(#[from] BankError),
}
