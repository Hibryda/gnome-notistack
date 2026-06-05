//! `org.freedesktop.Notifications` — FDO Desktop Notifications spec 1.2.
//!
//! M0 declares the full method/signal contract (rule 11: contract first); M4
//! implements behavior (hint decode, `replaces_id`/tombstone, expiry wiring).

use std::collections::HashMap;
use zbus::interface;
use zbus::object_server::SignalEmitter;
use zbus::zvariant::OwnedValue;

/// State handle for the FDO interface. M3+: shared access to the notification stack.
#[derive(Default)]
pub struct FdoNotifications {}

#[interface(name = "org.freedesktop.Notifications")]
impl FdoNotifications {
    /// Capabilities we advertise. M4: reconcile with actually-implemented features.
    fn get_capabilities(&self) -> Vec<String> {
        vec![
            "body".into(),
            "body-markup".into(),
            "actions".into(),
            "icon-static".into(),
            "persistence".into(),
        ]
    }

    /// `(name, vendor, version, spec_version)`.
    fn get_server_information(&self) -> (String, String, String, String) {
        (
            "gnome-notistack".into(),
            "hibryda".into(),
            env!("CARGO_PKG_VERSION").into(),
            "1.2".into(),
        )
    }

    /// `Notify` — returns the server-assigned id (`replaces_id` if non-zero and live).
    /// M4: full hint decode, tombstone-aware replace, per-notification expiry timer.
    #[allow(clippy::too_many_arguments)]
    fn notify(
        &self,
        app_name: String,
        replaces_id: u32,
        app_icon: String,
        summary: String,
        body: String,
        actions: Vec<String>,
        hints: HashMap<String, OwnedValue>,
        expire_timeout: i32,
    ) -> u32 {
        let _ = (
            app_name,
            replaces_id,
            app_icon,
            summary,
            body,
            actions,
            hints,
            expire_timeout,
        );
        // M4: enqueue into the stack and return a real (non-zero) id.
        0
    }

    /// Close a notification by id, emitting `NotificationClosed` (reason 3 = by call).
    fn close_notification(&self, id: u32) {
        let _ = id; // M4.
    }

    #[zbus(signal)]
    async fn notification_closed(
        emitter: &SignalEmitter<'_>,
        id: u32,
        reason: u32,
    ) -> zbus::Result<()>;

    #[zbus(signal)]
    async fn action_invoked(
        emitter: &SignalEmitter<'_>,
        id: u32,
        action_key: String,
    ) -> zbus::Result<()>;
}
