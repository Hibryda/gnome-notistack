//! Override-redirect ARGB32 window pool + EWMH hints (M2).
//!
//! Each popup is a `depth 32` TrueColor override-redirect window (`border_pixel(0)`
//! mandatory with a non-default visual; `colormap` AllocNone). EWMH per popup:
//! `_NET_WM_WINDOW_TYPE = {NOTIFICATION, UTILITY}`, `_NET_WM_STATE_ABOVE`,
//! `_NET_WM_BYPASS_COMPOSITOR = 2` (keep compositing so alpha works). Crucially,
//! `WM_TAKE_FOCUS` is removed from `WM_PROTOCOLS` so a popup can never
//! unfullscreen a fullscreen app.

use super::RenderMode;

/// Probe whether the cairo XCBSurface path is usable; otherwise fall back to
/// ImageSurface (plan risk R5). M2 implements the scratch-pixmap probe; until
/// then the safe fallback is assumed.
pub fn probe_render_mode() -> RenderMode {
    // M2: create a scratch ARGB32 pixmap and attempt cairo::XCBSurface::create.
    RenderMode::ImageSurface
}
