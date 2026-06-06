//! `org.freedesktop.Notifications` — FDO Desktop Notifications spec 1.2.
//!
//! M3 wires `Notify`/`CloseNotification` into the render stack. M4 adds full hint
//! decode (urgency, image-data), markup translation, tombstone-aware replace, and
//! the `NotificationClosed`/`ActionInvoked` signal emission.

use std::collections::HashMap;
use std::sync::atomic::{AtomicU32, Ordering};
use std::time::Instant;

use tokio::sync::mpsc::UnboundedSender;
use tracing::info;
use zbus::interface;
use zbus::object_server::SignalEmitter;
use zbus::zvariant::OwnedValue;

use zbus::zvariant::Value;

use crate::notification::{Action, Notification, NotificationId, RawImage, Urgency};
use crate::render::Command;

/// Serves the FDO interface; forwards notifications to the render thread.
pub struct FdoNotifications {
    tx: UnboundedSender<Command>,
    next_id: AtomicU32,
}

impl FdoNotifications {
    pub fn new(tx: UnboundedSender<Command>) -> Self {
        // Ids start at 1; 0 is never a valid notification id per the spec.
        Self {
            tx,
            next_id: AtomicU32::new(1),
        }
    }
}

/// Parse the flat FDO `actions` array `[key1, label1, key2, label2, ...]`.
fn parse_actions(flat: Vec<String>) -> Vec<Action> {
    flat.chunks_exact(2)
        .map(|pair| Action {
            key: pair[0].clone(),
            label: pair[1].clone(),
        })
        .collect()
}

/// Decode the `urgency` hint (byte 0/1/2); default Normal.
fn parse_urgency(hints: &HashMap<String, OwnedValue>) -> Urgency {
    match hints.get("urgency").and_then(|v| u8::try_from(v).ok()) {
        Some(0) => Urgency::Low,
        Some(2) => Urgency::Critical,
        _ => Urgency::Normal,
    }
}

/// Extract the `image-path` (or legacy `image_path`) hint as a string.
/// M4-remaining: the inline `image-data` `(iiibiiay)` hint.
fn parse_image_path(hints: &HashMap<String, OwnedValue>) -> Option<String> {
    hints
        .get("image-path")
        .or_else(|| hints.get("image_path"))
        .and_then(|v| String::try_from(v.clone()).ok())
        .filter(|s| !s.is_empty())
}

/// Extract a string hint by key, if non-empty.
fn string_hint(hints: &HashMap<String, OwnedValue>, key: &str) -> Option<String> {
    hints
        .get(key)
        .and_then(|v| String::try_from(v.clone()).ok())
        .filter(|s| !s.is_empty())
}

/// Decode the inline `image-data` `(iiibiiay)` hint (also legacy `icon_data`).
fn parse_image_data(hints: &HashMap<String, OwnedValue>) -> Option<RawImage> {
    let v = hints
        .get("image-data")
        .or_else(|| hints.get("image_data"))
        .or_else(|| hints.get("icon_data"))?;
    let Value::Structure(s) = &**v else {
        return None;
    };
    let f = s.fields();
    if f.len() < 7 {
        return None;
    }
    let bytes = match &f[6] {
        Value::Array(arr) => {
            let mut out = Vec::with_capacity(arr.len());
            for item in arr.iter() {
                out.push(u8::try_from(item).ok()?);
            }
            out
        }
        _ => return None,
    };
    Some(RawImage {
        width: i32::try_from(&f[0]).ok()?,
        height: i32::try_from(&f[1]).ok()?,
        rowstride: i32::try_from(&f[2]).ok()?,
        has_alpha: bool::try_from(&f[3]).ok()?,
        channels: i32::try_from(&f[5]).ok()?,
        bytes,
    })
}

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

    /// `Notify` — returns the server-assigned id (`replaces_id` if non-zero).
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
        let id = if replaces_id != 0 {
            replaces_id
        } else {
            self.next_id.fetch_add(1, Ordering::Relaxed)
        };

        let notification = Notification {
            id: NotificationId::Fdo(id),
            app_name,
            app_icon,
            image_path: parse_image_path(&hints),
            image_data: parse_image_data(&hints),
            summary,
            body,
            actions: parse_actions(actions),
            urgency: parse_urgency(&hints),
            sound_file: string_hint(&hints, "sound-file"),
            sound_name: string_hint(&hints, "sound-name"),
            suppress_sound: hints
                .get("suppress-sound")
                .and_then(|v| bool::try_from(v).ok())
                .unwrap_or(false),
            // -1 = use default; 0 = never expire; >0 = explicit ms.
            expire_timeout_ms: (expire_timeout >= 0).then_some(expire_timeout),
            created: Instant::now(),
        };

        info!(id, app = %notification.app_name, summary = %notification.summary, "FDO Notify");
        if self.tx.send(Command::Show(Box::new(notification))).is_err() {
            tracing::warn!("render thread gone; dropping notification");
        }
        id
    }

    /// Close a notification by id. M4 also emits `NotificationClosed(id, 3)`.
    fn close_notification(&self, id: u32) {
        info!(id, "FDO CloseNotification");
        let _ = self.tx.send(Command::Close(NotificationId::Fdo(id)));
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

/// Public wrappers so the async signal-emitter task (outside the interface impl)
/// can emit these signals via a `SignalEmitter` bound to the FDO object path.
pub async fn emit_closed(emitter: &SignalEmitter<'_>, id: u32, reason: u32) -> zbus::Result<()> {
    FdoNotifications::notification_closed(emitter, id, reason).await
}

pub async fn emit_action(emitter: &SignalEmitter<'_>, id: u32, key: String) -> zbus::Result<()> {
    FdoNotifications::action_invoked(emitter, id, key).await
}
