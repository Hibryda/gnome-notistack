//! `org.gtk.Notifications` — GNOME's private notification interface (M7).
//!
//! Native GTK/GIO apps (`g_application_send_notification`) and Flatpaks (via the
//! portal) route here. Differs from FDO: string `(app_id, id)` identity, a
//! priority string, and `app.`-prefixed actions dispatched via
//! `org.freedesktop.Application.ActivateAction` (M7.1 — not yet wired; for now
//! GTK popups display and dismiss). The icon GIcon variant is also M7.1; we fall
//! back to a themed lookup of the `app_id` (usually the desktop/icon id).

use std::collections::HashMap;
use std::time::Instant;

use tokio::sync::mpsc::UnboundedSender;
use tracing::info;
use zbus::interface;
use zbus::zvariant::OwnedValue;

use crate::notification::{Action, Notification, NotificationId, Urgency};
use crate::render::Command;

pub struct GtkNotifications {
    tx: UnboundedSender<Command>,
}

impl GtkNotifications {
    pub fn new(tx: UnboundedSender<Command>) -> Self {
        Self { tx }
    }
}

fn str_field(map: &HashMap<String, OwnedValue>, key: &str) -> Option<String> {
    map.get(key).and_then(|v| String::try_from(v.clone()).ok())
}

/// Parse the GTK `buttons` array (`aa{sv}` of `label`/`action`/`target`) into
/// action buttons. The `action` is an `app.`-prefixed name dispatched on click.
fn parse_buttons(map: &HashMap<String, OwnedValue>) -> Vec<Action> {
    let Some(v) = map.get("buttons") else {
        return Vec::new();
    };
    let Ok(arr) = Vec::<HashMap<String, OwnedValue>>::try_from(v.clone()) else {
        return Vec::new();
    };
    arr.iter()
        .filter_map(|b| {
            Some(Action {
                key: str_field(b, "action")?,
                label: str_field(b, "label")?,
            })
        })
        .collect()
}

#[interface(name = "org.gtk.Notifications")]
impl GtkNotifications {
    /// `AddNotification(app_id, id, a{sv})`. The dict carries `title`, `body`,
    /// `priority`, `icon`, `default-action`, `buttons`. M7 maps the text +
    /// priority; buttons/icon/action-dispatch are M7.1.
    fn add_notification(
        &self,
        app_id: String,
        id: String,
        notification: HashMap<String, OwnedValue>,
    ) {
        let urgency = match str_field(&notification, "priority").as_deref() {
            Some("urgent") => Urgency::Critical,
            Some("low") => Urgency::Low,
            _ => Urgency::Normal,
        };
        let n = Notification {
            id: NotificationId::Gtk {
                app_id: app_id.clone(),
                id: id.clone(),
            },
            app_name: app_id.clone(),
            // Heuristic icon: the GTK app_id is usually the themed icon name too.
            app_icon: app_id.clone(),
            image_path: None,
            image_data: None,
            summary: str_field(&notification, "title").unwrap_or_default(),
            body: str_field(&notification, "body").unwrap_or_default(),
            actions: parse_buttons(&notification),
            // Whole-card click invokes the GTK default-action.
            default_action: str_field(&notification, "default-action"),
            urgency,
            sound_file: None,
            sound_name: None,
            suppress_sound: false,
            expire_timeout_ms: None,
            created: Instant::now(),
        };
        info!(app_id, id, "GTK AddNotification");
        let _ = self.tx.send(Command::Show(Box::new(n)));
    }

    /// `RemoveNotification(app_id, id)`.
    fn remove_notification(&self, app_id: String, id: String) {
        info!(app_id, id, "GTK RemoveNotification");
        let _ = self
            .tx
            .send(Command::Close(NotificationId::Gtk { app_id, id }));
    }
}
