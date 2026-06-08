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
/// `label` is parsed and stored for the deferred button-rendering pass
/// (docs/known-loss.md); whole-card default-action dispatch already works.
#[derive(Debug, Clone)]
pub struct Action {
    pub key: String,
    pub label: String,
}

/// Raw inline icon from the FDO `image-data` hint `(iiibiiay)` or GTK GIcon bytes.
/// Cap on inline image dimensions (bounds decode cost; plan risk R15).
pub const MAX_RAW_DIM: usize = 4096;

/// A validated inline `image-data` buffer. Private fields + the fallible
/// [`RawImage::from_wire`] constructor are the *only* way to build one, so every
/// holder is guaranteed renderable — the untrusted-D-Bus bounds checks live at
/// the boundary, not at each use site (rule 01/02).
#[derive(Debug, Clone)]
pub struct RawImage {
    width: usize,
    height: usize,
    channels: usize,
    rowstride: usize,
    bytes: Vec<u8>,
}

impl RawImage {
    /// Build from the raw FDO `(iiibiiay)` fields, rejecting malformed/hostile
    /// wire data (non-positive or oversized dims, bad channel count, a stride
    /// shorter than a row, or a truncated buffer). Uses checked arithmetic so a
    /// negative/huge `rowstride` can't overflow (which panics on debug builds).
    pub fn from_wire(
        width: i32,
        height: i32,
        rowstride: i32,
        channels: i32,
        bytes: Vec<u8>,
    ) -> Option<Self> {
        let w = usize::try_from(width).ok()?;
        let h = usize::try_from(height).ok()?;
        let ch = usize::try_from(channels).ok()?;
        let st = usize::try_from(rowstride).ok()?;
        if w == 0 || h == 0 || w > MAX_RAW_DIM || h > MAX_RAW_DIM || !(3..=4).contains(&ch) {
            return None;
        }
        let row_bytes = w.checked_mul(ch)?;
        if st < row_bytes || bytes.len() < st.checked_mul(h)? {
            return None;
        }
        Some(Self {
            width: w,
            height: h,
            channels: ch,
            rowstride: st,
            bytes,
        })
    }

    pub fn width(&self) -> usize {
        self.width
    }
    pub fn height(&self) -> usize {
        self.height
    }
    pub fn channels(&self) -> usize {
        self.channels
    }
    /// Row `y` of the (validated) pixel buffer; always in bounds for `y < height`.
    pub fn row(&self, y: usize) -> &[u8] {
        &self.bytes[y * self.rowstride..]
    }
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
    /// Action buttons (Fdo `actions` pairs / GTK `buttons`), rendered as buttons.
    pub actions: Vec<Action>,
    /// Action invoked on a whole-card click: Fdo `"default"` key, or a GTK
    /// `app.`-prefixed action name. `None` if the notification has no default.
    pub default_action: Option<String>,
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Instant;

    fn buf(n: usize) -> Vec<u8> {
        vec![0u8; n]
    }

    #[test]
    fn from_wire_rejects_bad_dimensions() {
        assert!(RawImage::from_wire(0, 2, 8, 4, buf(16)).is_none());
        assert!(RawImage::from_wire(2, 0, 8, 4, buf(16)).is_none());
        assert!(RawImage::from_wire(-1, 2, 8, 4, buf(16)).is_none());
        // > MAX_RAW_DIM
        assert!(RawImage::from_wire(5000, 2, 20000, 4, buf(40000)).is_none());
    }

    #[test]
    fn from_wire_rejects_bad_channels() {
        assert!(RawImage::from_wire(2, 2, 8, 2, buf(16)).is_none());
        assert!(RawImage::from_wire(2, 2, 8, 5, buf(16)).is_none());
    }

    #[test]
    fn from_wire_rejects_short_stride_or_truncated() {
        assert!(RawImage::from_wire(10, 2, 10, 4, buf(80)).is_none()); // stride < 10*4
        assert!(RawImage::from_wire(2, 2, 8, 4, buf(15)).is_none()); // bytes < stride*h
    }

    #[test]
    fn from_wire_rejects_negative_rowstride_without_panic() {
        // -1 as usize is huge; checked arithmetic must reject, not overflow-panic.
        assert!(RawImage::from_wire(2, 2, -1, 4, buf(16)).is_none());
        assert!(RawImage::from_wire(2, 2, i32::MAX, 4, buf(16)).is_none());
    }

    #[test]
    fn from_wire_accepts_valid() {
        let rgb = RawImage::from_wire(2, 2, 6, 3, buf(12)).unwrap();
        assert_eq!((rgb.width(), rgb.height(), rgb.channels()), (2, 2, 3));
        // Padded stride (real GTK pixbufs) must be honored, not rejected.
        let rgba = RawImage::from_wire(2, 2, 16, 4, buf(32)).unwrap();
        assert_eq!(rgba.row(1).len(), 16);
    }

    fn notif(urgency: Urgency, expire: Option<i32>) -> Notification {
        Notification {
            id: NotificationId::Fdo(1),
            app_name: String::new(),
            app_icon: String::new(),
            image_path: None,
            image_data: None,
            summary: String::new(),
            body: String::new(),
            actions: vec![],
            default_action: None,
            urgency,
            sound_file: None,
            sound_name: None,
            suppress_sound: false,
            expire_timeout_ms: expire,
            created: Instant::now(),
        }
    }

    #[test]
    fn auto_expires_rules() {
        assert!(notif(Urgency::Normal, None).auto_expires());
        assert!(notif(Urgency::Normal, Some(5000)).auto_expires());
        assert!(!notif(Urgency::Normal, Some(0)).auto_expires()); // sticky
        assert!(!notif(Urgency::Critical, None).auto_expires()); // never
        assert!(!notif(Urgency::Critical, Some(5000)).auto_expires()); // urgency wins
    }
}
