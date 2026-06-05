//! ARGB32 override-redirect rendering (M2 windows, M4 card drawing).
//!
//! One override-redirect ARGB32 window per popup; cairo/pango drawing; y-cursor
//! layout; freedesktop icon assets.

pub mod assets;
pub mod cairo;
pub mod layout;
pub mod window;

/// Chosen once at startup by probing (plan risk R5): the cairo XCBSurface fast
/// path (the one load-bearing unsafe seam) or the safe ImageSurface + put_image
/// fallback. A single decision, not per-window.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RenderMode {
    XcbSurface,
    ImageSurface,
}
