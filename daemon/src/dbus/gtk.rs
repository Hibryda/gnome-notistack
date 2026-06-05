//! `org.gtk.Notifications` — GNOME's private notification interface (M6/M7).
//!
//! Conditional on the M0.5 live-spike GO. Differs from FDO: string `(app_id, id)`
//! identity, `app.`-prefixed actions dispatched via
//! `org.freedesktop.Application.ActivateAction` (strip the `app.` prefix first,
//! plan OBJ-57), and app-exit persistence.

use std::collections::HashMap;
use tracing::info;
use zbus::interface;
use zbus::zvariant::OwnedValue;

#[derive(Default)]
pub struct GtkNotifications {}

#[interface(name = "org.gtk.Notifications")]
impl GtkNotifications {
    /// `AddNotification(app_id, id, notification)` — the `a{sv}` carries title,
    /// body, icon, priority, and `buttons` (each with `action` + `target`). M7.
    fn add_notification(
        &self,
        app_id: String,
        id: String,
        notification: HashMap<String, OwnedValue>,
    ) {
        info!(
            app_id,
            id,
            keys = notification.len(),
            "GTK AddNotification received"
        );
    }

    /// `RemoveNotification(app_id, id)`. M7.
    fn remove_notification(&self, app_id: String, id: String) {
        info!(app_id, id, "GTK RemoveNotification received");
    }
}
