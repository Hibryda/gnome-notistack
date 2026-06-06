//! Private control interface for the `notistack-ctl` companion and the shell
//! extension's handshake telemetry (M1+).
//!
//! Security (rule 01): mutating control methods must re-query the caller PID via
//! `GetConnectionUnixProcessID` per call (plan OBJ / risk R10) — never trust a
//! cached PID. (PID gating is M6.1; the bus name itself is session-scoped.)

use std::time::Duration;

use tracing::{info, warn};
use zbus::fdo::RequestNameFlags;
use zbus::interface;

use crate::dbus;

/// Re-request both notification names (REPLACE_EXISTING → queued behind the
/// current owner) so a future takeover has a claimant.
async fn requeue(conn: &zbus::Connection) {
    let flags = RequestNameFlags::ReplaceExisting.into();
    for name in [dbus::FDO_NAME, dbus::GTK_NAME] {
        match conn.request_name_with_flags(name, flags).await {
            Ok(reply) => info!(name, ?reply, "re-queued notification name"),
            Err(e) => warn!(name, error = %e, "requeue failed"),
        }
    }
}

#[derive(Default)]
pub struct Control {}

#[interface(name = "store.hemoglobina.notistack.Control")]
impl Control {
    /// Liveness probe used by the extension's `enable()` fast-path (plan risk R4).
    fn is_ready(&self) -> bool {
        true
    }

    /// The extension reports handshake progress/errors here (typed vocabulary, M1).
    fn report_handshake_event(&self, kind: String, detail: String) {
        let _ = (kind, detail); // M1: structured tracing + state transitions.
    }

    /// Release the notification names so the shell can reclaim them on extension
    /// `disable()` — without stopping the daemon. The extension then re-owns the
    /// GTK name and reactivates the gjs Fdo proxy.
    ///
    /// `release_name` also drops a *queued* (non-owner) request, so after releasing
    /// we re-queue (after a delay, so the shell/proxy reclaim first and we queue
    /// behind them). Without this, toggling the extension off→on would leave the
    /// daemon de-queued and the freed names unclaimed.
    async fn relinquish(&self, #[zbus(connection)] conn: &zbus::Connection) {
        for name in [dbus::FDO_NAME, dbus::GTK_NAME] {
            match conn.release_name(name).await {
                Ok(released) => info!(name, released, "relinquished notification name"),
                Err(e) => warn!(name, error = %e, "relinquish failed"),
            }
        }
        let conn = conn.clone();
        tokio::spawn(async move {
            tokio::time::sleep(Duration::from_secs(1)).await;
            requeue(&conn).await;
        });
    }
}
