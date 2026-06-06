//! Metadata-only notification history (M5).
//!
//! A bounded ring buffer persisted to `$XDG_STATE_HOME/gnome-notistack/history.json`
//! (default `~/.local/state/...`). We store metadata only — no decoded image
//! payloads — to bound disk/memory. Used by a future `notistack-ctl history`.

use std::collections::VecDeque;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use tracing::{debug, warn};

use crate::notification::Notification;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HistoryEntry {
    pub app_name: String,
    pub summary: String,
    pub body: String,
}

pub struct History {
    entries: VecDeque<HistoryEntry>,
    cap: usize,
    path: Option<PathBuf>,
}

impl History {
    /// Load existing history (best-effort), keeping at most `cap` entries.
    pub fn load(cap: usize) -> Self {
        let path = Self::path();
        let entries = path
            .as_ref()
            .and_then(|p| std::fs::read_to_string(p).ok())
            .and_then(|s| serde_json::from_str::<Vec<HistoryEntry>>(&s).ok())
            .map(|v| v.into_iter().rev().take(cap).rev().collect())
            .unwrap_or_default();
        Self { entries, cap, path }
    }

    /// Record a notification and persist (best-effort; never blocks display).
    pub fn record(&mut self, n: &Notification) {
        self.entries.push_back(HistoryEntry {
            app_name: n.app_name.clone(),
            summary: n.summary.clone(),
            body: n.body.clone(),
        });
        while self.entries.len() > self.cap {
            self.entries.pop_front();
        }
        self.save();
    }

    fn save(&self) {
        let Some(path) = &self.path else { return };
        if let Some(parent) = path.parent() {
            if let Err(e) = std::fs::create_dir_all(parent) {
                warn!(error = %e, "creating history dir");
                return;
            }
        }
        let entries: Vec<&HistoryEntry> = self.entries.iter().collect();
        match serde_json::to_string(&entries) {
            Ok(json) => {
                if let Err(e) = std::fs::write(path, json) {
                    warn!(error = %e, "writing history");
                }
            }
            Err(e) => debug!(error = %e, "serializing history"),
        }
    }

    fn path() -> Option<PathBuf> {
        let base = std::env::var_os("XDG_STATE_HOME")
            .map(PathBuf::from)
            .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".local/state")))?;
        Some(base.join("gnome-notistack").join("history.json"))
    }
}
