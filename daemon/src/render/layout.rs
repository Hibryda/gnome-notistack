//! Card layout + click regions (M4).
//!
//! Computes geometry for the icon, summary, body, and action buttons of a popup,
//! and exports the clickable regions consumed by `event_loop` for hit-testing.

/// A clickable region within a popup, mapped to an action or affordance.
#[derive(Debug, Clone)]
pub struct ClickRegion {
    pub x: i16,
    pub y: i16,
    pub width: u16,
    pub height: u16,
    pub target: ClickTarget,
}

#[derive(Debug, Clone)]
pub enum ClickTarget {
    /// Invoke a named action (Fdo action key, or GTK `app.`-stripped name).
    Action(String),
    /// Dismiss the popup.
    Close,
    /// The default action (whole-card click).
    Default,
}
