//! Icon resolution + decode (M4).
//!
//! Lookup priority: inline `image-data` hint `(iiibiiay)` > `image-path` >
//! themed `app_icon` via freedesktop-icons > placeholder. PNG/JPEG decode via
//! the `image` crate; SVG via `librsvg-rebind`. A 512×512 cap bounds decode cost
//! and guards against `image-data` DoS (plan risk R15). Cached by notification id.
