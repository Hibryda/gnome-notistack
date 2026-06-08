//! DND + lock suppression watcher (M5).
//!
//! Polls (1 s) two signals and tells the render thread when to suppress:
//!   - **DND**: gsettings `org.gnome.desktop.notifications show-banners` = false.
//!   - **Lock**: `org.gnome.ScreenSaver.GetActive` (plan OBJ-39 — the reliable
//!     source under GNOME, not the X11 screensaver extension).
//!
//! `suppress = DND || locked`. While suppressed the render thread queues
//! notifications and replays them on unsuppress, so nothing is lost. Fullscreen
//! suppression is a later addition. Polling (not signals) keeps this dependency-
//! light; 1 s latency for DND/lock changes is imperceptible.

use std::time::Duration;

use tokio::sync::mpsc::UnboundedSender;
use tracing::{debug, info};

use crate::render::Command;

pub async fn run(conn: zbus::Connection, tx: UnboundedSender<Command>) {
    let mut last = false;
    let mut ticker = tokio::time::interval(Duration::from_secs(1));
    loop {
        ticker.tick().await;
        // show-banners absent/unreadable => assume banners on (not DND).
        let dnd = !show_banners().await.unwrap_or(true);
        let locked = screen_locked(&conn).await.unwrap_or(false);
        let suppressed = dnd || locked;
        if suppressed != last {
            info!(dnd, locked, suppressed, "suppression state changed");
            if tx.send(Command::SetSuppressed(suppressed)).is_err() {
                break; // render thread gone
            }
            last = suppressed;
        }
    }
}

/// Read `org.gnome.desktop.notifications show-banners` via gsettings.
async fn show_banners() -> Option<bool> {
    let out = tokio::process::Command::new("gsettings")
        .args(["get", "org.gnome.desktop.notifications", "show-banners"])
        .output()
        .await
        .ok()?;
    match String::from_utf8_lossy(&out.stdout).trim() {
        "true" => Some(true),
        "false" => Some(false),
        other => {
            debug!(value = other, "unexpected show-banners value");
            None
        }
    }
}

/// Query the GNOME screensaver lock state.
async fn screen_locked(conn: &zbus::Connection) -> Option<bool> {
    let proxy = zbus::Proxy::new(
        conn,
        "org.gnome.ScreenSaver",
        "/org/gnome/ScreenSaver",
        "org.gnome.ScreenSaver",
    )
    .await
    .ok()?;
    // Bound the call (rule 14): a hung screensaver service must not stall the
    // suppression poll loop. On timeout, treat as "unknown" (not locked).
    let call = proxy.call("GetActive", &());
    match tokio::time::timeout(std::time::Duration::from_secs(2), call).await {
        Ok(r) => r.ok(),
        Err(_) => {
            tracing::warn!("org.gnome.ScreenSaver GetActive timed out");
            None
        }
    }
}
