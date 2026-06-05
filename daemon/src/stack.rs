//! Notification stack manager (M3): the 3-state queue (waiting / displayed /
//! history), y-cursor reflow, per-notification expiry timers, and the
//! `replaces_id` + tombstone bookkeeping.

use crate::notification::{Notification, NotificationId};
use std::collections::HashMap;
use tokio::task::JoinHandle;

/// A displayed/queued notification plus its lifecycle handles.
pub struct NotificationEntry {
    pub notification: Notification,
    /// Per-notification expiry timer (M3). Critical urgency / sticky => `None`.
    pub expiry_handle: Option<JoinHandle<()>>,
}

#[derive(Default)]
pub struct Stack {
    displayed: Vec<NotificationEntry>,
    waiting: Vec<NotificationEntry>,
    /// Recently-closed ids: a `replaces_id` targeting a tombstoned id must create
    /// a fresh notification rather than silently updating a gone one (plan OBJ-51).
    tombstones: HashMap<NotificationId, ()>,
}

impl Stack {
    pub fn new() -> Self {
        Self::default()
    }

    /// Number of currently displayed popups.
    pub fn displayed_len(&self) -> usize {
        self.displayed.len()
    }
}
