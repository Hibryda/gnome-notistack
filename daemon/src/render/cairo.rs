//! The load-bearing unsafe seam (M2/M4).
//!
//! A cairo `XCBSurface` is created against x11rb's `XCBConnection` raw pointer
//! (`get_raw_xcb_connection()`). This is the single `unsafe` block in the crate;
//! it is gated behind a startup probe (`render::window::probe_render_mode`) and
//! falls back to `ImageSurface` + `put_image` when the probe fails (plan risk R5).
