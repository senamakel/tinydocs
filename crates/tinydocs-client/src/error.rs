//! TinyDocs call errors kept distinct for the host's tool and reporting layers.

use std::fmt::{Debug, Display, Formatter};

/// Why a TinyDocs operation did not produce its requested result.
///
/// Remote text is retained in the value for an intentional caller-facing error
/// path. Its [`Debug`] form redacts the text so diagnostics do not accidentally
/// include module-provided data.
#[derive(Clone, PartialEq, Eq)]
pub enum DocumentCallError {
    /// The module is unavailable or cannot serve this contract.
    Unavailable(String),
    /// TinyDocs rejected caller-provided input.
    InvalidInput(String),
    /// The operation or its output transfer failed.
    Failed(String),
}

impl Debug for DocumentCallError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        let (kind, message) = match self {
            Self::Unavailable(_) => ("Unavailable", "<redacted>"),
            Self::InvalidInput(_) => ("InvalidInput", "<redacted>"),
            Self::Failed(_) => ("Failed", "<redacted>"),
        };
        formatter.debug_tuple(kind).field(&message).finish()
    }
}

impl Display for DocumentCallError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        let message = match self {
            Self::Unavailable(message) | Self::InvalidInput(message) | Self::Failed(message) => {
                message
            }
        };
        formatter.write_str(message)
    }
}

impl std::error::Error for DocumentCallError {}
