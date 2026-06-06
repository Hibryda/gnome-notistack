//! Session-bus wiring + queue-based name acquisition (M1).
//!
//! Serves the FDO interface (+ control iface, + GTK iface when enabled) and
//! requests the well-known names with the audit invariant: `REPLACE_EXISTING`
//! only — never `ALLOW_REPLACEMENT`, never `DO_NOT_QUEUE`. Against the current
//! owners (which hold the names without `ALLOW_REPLACEMENT`) this lands us
//! `InQueue`; D-Bus promotes us to owner the moment the companion extension
//! releases the shell's ownership. Nothing here perturbs the running shell.

use anyhow::Context;
use tokio::sync::mpsc::UnboundedSender;
use tracing::{info, warn};
use zbus::fdo::RequestNameFlags;

use crate::config::Config;
use crate::dbus::{self, fdo::FdoNotifications, gtk::GtkNotifications, private::Control};
use crate::render::Command;

/// Object path for our private control interface.
pub const CONTROL_PATH: &str = "/store/hemoglobina/notistack/Control";

/// Connect, serve the interfaces, and queue for the notification name(s).
/// Returns the live connection (kept alive by the caller for the daemon's life).
/// `tx` forwards incoming notifications to the render thread.
pub async fn serve(
    config: &Config,
    tx: UnboundedSender<Command>,
) -> anyhow::Result<zbus::Connection> {
    let conn = zbus::connection::Builder::session()
        .context("connecting to session bus")?
        .serve_at(dbus::FDO_PATH, FdoNotifications::new(tx.clone()))
        .context("exporting FDO interface")?
        .serve_at(CONTROL_PATH, Control::default())
        .context("exporting control interface")?
        .build()
        .await
        .context("building D-Bus connection")?;

    request_name(&conn, dbus::FDO_NAME).await;

    if config.gtk_takeover {
        conn.object_server()
            .at(dbus::GTK_PATH, GtkNotifications::new(tx))
            .await
            .context("exporting GTK interface")?;
        request_name(&conn, dbus::GTK_NAME).await;
    } else {
        info!("gtk_takeover disabled — running Fdo-only (partial coverage)");
    }

    Ok(conn)
}

/// Request a well-known name with the queue invariant (REPLACE_EXISTING only).
async fn request_name(conn: &zbus::Connection, name: &str) {
    let flags = RequestNameFlags::ReplaceExisting.into();
    match conn.request_name_with_flags(name, flags).await {
        Ok(reply) => info!(name, ?reply, "requested notification name"),
        Err(e) => warn!(name, error = %e, "name request failed"),
    }
}
