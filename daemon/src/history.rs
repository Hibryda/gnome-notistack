//! Metadata-only notification history (M5).
//!
//! A bounded ring buffer persisted to `~/.local/state/gnome-notistack/history.json`.
//! We store metadata only (no decoded image payloads) to bound disk/memory.

#[derive(Debug, Clone)]
pub struct HistoryEntry {
    pub app_name: String,
    pub summary: String,
    pub body: String,
}
