use std::error::Error;
use std::fmt::{self, Display};
use std::path::PathBuf;

/// Wrappers for the different errors that can be encountered while running the `Editor`.
#[derive(Debug)]
pub enum EditorError {
    /// Some kind of unexpected File IO error.
    FileIo(std::io::Error),
    /// Some kind of unexpected IO error when dealing with the TTY.
    TermIo(crossterm::ErrorKind),
    /// Occurs when trying to create a new file in a directory that doesn't exist.
    DirectoryDoesNotExist(PathBuf),
    /// Occurs when trying to open the path `/`
    CannotOpenRoot,
}

impl Display for EditorError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> fmt::Result {
        match self {
            EditorError::FileIo(e) => write!(f, "Encountered error when performing file IO: {}", e),
            EditorError::TermIo(e) => {
                write!(f, "Encountered error when performing terminal IO: {}", e)
            }
            EditorError::DirectoryDoesNotExist(p) => {
                write!(
                    f,
                    "The directory path {} does not exist",
                    p.to_str().unwrap()
                )
            }
            EditorError::CannotOpenRoot => {
                write!(f, "Cannot open the path \"/\"")
            }
        }
    }
}

impl Error for EditorError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            EditorError::FileIo(e) => Some(e),
            EditorError::TermIo(e) => Some(e),
            EditorError::DirectoryDoesNotExist(_) => None,
            EditorError::CannotOpenRoot => None,
        }
    }
}

pub type Result<T> = std::result::Result<T, EditorError>;
