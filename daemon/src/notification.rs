//! Protocol-independent notification model. Both the Fdo (UINT32 id) and GTK
//! (string `(app_id, id)` tuple) interfaces map onto these types so the stack,
//! renderer, and history never care which wire protocol delivered a notification.

use std::time::Instant;

/// Stable internal identity, independent of the wire protocol.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum NotificationId {
    /// `org.freedesktop.Notifications` — server-assigned u32 (never 0).
    Fdo(u32),
    /// `org.gtk.Notifications` — `(app_id, app-chosen id)`.
    Gtk { app_id: String, id: String },
}

/// FDO urgency hint (0/1/2). Critical never auto-expires.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Default)]
pub enum Urgency {
    Low = 0,
    #[default]
    Normal = 1,
    Critical = 2,
}

/// An actionable button. Fdo: `(key, label)` pairs from the `actions` array.
/// GTK: `app.`-prefixed action names invoked via `org.freedesktop.Application.ActivateAction`.
#[derive(Debug, Clone)]
pub struct Action {
    pub key: String,
    pub label: String,
}

/// Raw inline icon from the FDO `image-data` hint `(iiibiiay)` or GTK GIcon bytes.
#[derive(Debug, Clone)]
pub struct RawImage {
    pub width: i32,
    pub height: i32,
    pub rowstride: i32,
    pub has_alpha: bool,
    pub channels: i32,
    pub bytes: Vec<u8>,
}

#[derive(Debug, Clone)]
pub struct Notification {
    pub id: NotificationId,
    pub app_name: String,
    pub app_icon: String,
    /// `image-path` hint (a file path or `file://` URI), if provided.
    pub image_path: Option<String>,
    /// Inline `image-data` hint pixels, if provided (highest icon priority).
    pub image_data: Option<RawImage>,
    pub summary: String,
    pub body: String,
    pub actions: Vec<Action>,
    pub urgency: Urgency,
    /// `sound-file` hint (a path to an audio file), if provided.
    pub sound_file: Option<String>,
    /// `sound-name` hint (a themed sound name), if provided.
    pub sound_name: Option<String>,
    /// `suppress-sound` hint: caller asked for no sound.
    pub suppress_sound: bool,
    /// `None` = use the config default; `Some(0)` = never expire (sticky/critical).
    pub expire_timeout_ms: Option<i32>,
    pub created: Instant,
}

impl Notification {
    /// Whether this notification should auto-expire (false for critical or sticky).
    pub fn auto_expires(&self) -> bool {
        self.urgency != Urgency::Critical && self.expire_timeout_ms != Some(0)
    }
}
