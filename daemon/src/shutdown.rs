//! Graceful shutdown (rule 14, 12-Factor IX).
//!
//! On SIGTERM the systemd unit's `ExecStopPost` performs the override-file
//! cleanup + `daemon-reload` (see `packaging/`), so it survives even SIGKILL.
//! The in-process handler therefore only needs to release local resources
//! (M2+: tear down the X11 windows and drain in-flight work).

use tokio::signal::unix::{signal, SignalKind};

/// Resolve when the daemon receives SIGTERM or SIGINT.
pub async fn wait_for_shutdown() {
    let mut sigterm = signal(SignalKind::terminate()).expect("install SIGTERM handler");
    let mut sigint = signal(SignalKind::interrupt()).expect("install SIGINT handler");

    tokio::select! {
        _ = sigterm.recv() => {}
        _ = sigint.recv() => {}
    }
}
