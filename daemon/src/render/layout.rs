//! Card layout + click regions.
//!
//! Scaffolding for **per-button** click regions (deferred button rendering — see
//! docs/known-loss.md). Whole-card click + default-action dispatch already work
//! in `render::manager`; these types are consumed once buttons are drawn.
#![allow(dead_code)]

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
