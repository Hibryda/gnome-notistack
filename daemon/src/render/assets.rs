//! Icon resolution + decode (M4).
//!
//! Resolution priority: `image-path` hint > `app_icon` (a path, or a themed name
//! resolved via freedesktop-icons) > none. PNG/JPEG decode via the `image` crate.
//! Inline `image-data` `(iiibiiay)` hint decode and SVG (librsvg/resvg) are
//! M4-remaining (see docs/IMPLEMENTATION-PLAN.md §7). A 512px cap bounds decode
//! cost (plan risk R15).

use std::path::PathBuf;

use image::imageops::FilterType;
use tracing::debug;

/// Resolve and decode an icon to premultiplied BGRA (cairo ARGB32 byte order) at
/// `size`×`size`. Returns `(pixels, size)`, or `None` if no icon resolves/decodes.
pub fn load_icon(app_icon: &str, image_path: Option<&str>, size: u32) -> Option<(Vec<u8>, i32)> {
    let size = size.min(512);
    let path = resolve(app_icon, image_path, size)?;
    let img = match image::open(&path) {
        Ok(i) => i,
        Err(e) => {
            debug!(path = %path.display(), error = %e, "icon decode failed");
            return None;
        }
    };
    let rgba = img
        .resize_exact(size, size, FilterType::Lanczos3)
        .to_rgba8();
    let mut data = rgba.into_raw();
    premultiply_bgra(&mut data);
    Some((data, size as i32))
}

fn resolve(app_icon: &str, image_path: Option<&str>, size: u32) -> Option<PathBuf> {
    if let Some(p) = image_path.and_then(file_path) {
        return Some(p);
    }
    if !app_icon.is_empty() {
        if let Some(p) = file_path(app_icon) {
            return Some(p);
        }
        return freedesktop_icons::lookup(app_icon)
            .with_size(size as u16)
            .find();
    }
    None
}

/// Treat a string as a filesystem path (stripping a `file://` prefix); returns it
/// only if it points at an existing file.
fn file_path(s: &str) -> Option<PathBuf> {
    let p = s.strip_prefix("file://").unwrap_or(s);
    let pb = PathBuf::from(p);
    pb.is_file().then_some(pb)
}

/// Convert non-premultiplied RGBA (image crate) to premultiplied BGRA in place
/// (cairo ARGB32 native byte order on little-endian).
fn premultiply_bgra(data: &mut [u8]) {
    for px in data.chunks_exact_mut(4) {
        let (r, g, b, a) = (px[0], px[1], px[2], px[3]);
        let m = |c: u8| ((c as u16 * a as u16 + 127) / 255) as u8;
        px[0] = m(b);
        px[1] = m(g);
        px[2] = m(r);
        px[3] = a;
    }
}
