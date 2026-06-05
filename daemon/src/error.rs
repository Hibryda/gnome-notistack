//! Daemon error type. D-Bus method handlers map these to the appropriate
//! protocol error replies; internal logs keep full context (rules 02, 13).

use thiserror::Error;

#[derive(Debug, Error)]
pub enum Error {
    #[error("X11 error: {0}")]
    X11(String),

    #[error("D-Bus error: {0}")]
    DBus(#[from] zbus::Error),

    #[error("invalid notification: {0}")]
    InvalidNotification(String),

    #[error("rendering error: {0}")]
    Render(String),

    #[error(transparent)]
    Io(#[from] std::io::Error),
}

pub type Result<T> = std::result::Result<T, Error>;
