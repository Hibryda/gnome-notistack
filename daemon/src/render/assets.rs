//! Icon resolution + decode (M4).
//!
//! Resolution priority: inline `image-data` hint > `image-path` hint > `app_icon`
//! (a path, or a themed name via freedesktop-icons) > none. PNG/JPEG via the
//! `image` crate, SVG via `resvg` (pure-Rust). All outputs are premultiplied
//! BGRA (cairo ARGB32 byte order) at `size`×`size`. A 512px cap bounds decode
//! cost and `image-data` dimensions are sanity-capped (plan risk R15).

use std::path::{Path, PathBuf};

use image::imageops::FilterType;
use resvg::{tiny_skia, usvg};
use tracing::debug;

use crate::notification::RawImage;

/// Resolve and decode an icon to premultiplied BGRA at `size`×`size`. `theme` is
/// the active icon theme (themed lookups inherit down to hicolor).
pub fn load_icon(
    app_icon: &str,
    image_path: Option<&str>,
    image_data: Option<&RawImage>,
    size: u32,
    theme: &str,
) -> Option<(Vec<u8>, i32)> {
    let size = size.min(512);
    if let Some(raw) = image_data {
        if let Some(out) = raw_to_bgra(raw, size) {
            return Some(out);
        }
    }
    let path = resolve(app_icon, image_path, size, theme)?;
    decode_path(&path, size)
}

fn resolve(app_icon: &str, image_path: Option<&str>, size: u32, theme: &str) -> Option<PathBuf> {
    if let Some(p) = image_path.and_then(file_path) {
        return Some(p);
    }
    if !app_icon.is_empty() {
        if let Some(p) = file_path(app_icon) {
            return Some(p);
        }
        return freedesktop_icons::lookup(app_icon)
            .with_size(size as u16)
            .with_theme(theme)
            .find();
    }
    None
}

fn decode_path(path: &Path, size: u32) -> Option<(Vec<u8>, i32)> {
    let is_svg = matches!(
        path.extension().and_then(|e| e.to_str()),
        Some("svg") | Some("svgz")
    );
    if is_svg {
        return render_svg(path, size);
    }
    let img = match image::open(path) {
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

/// Render an SVG to premultiplied BGRA, scaled to fit `size`×`size`.
fn render_svg(path: &Path, size: u32) -> Option<(Vec<u8>, i32)> {
    let bytes = std::fs::read(path).ok()?;
    let tree = match usvg::Tree::from_data(&bytes, &usvg::Options::default()) {
        Ok(t) => t,
        Err(e) => {
            debug!(path = %path.display(), error = %e, "svg parse failed");
            return None;
        }
    };
    let mut pixmap = tiny_skia::Pixmap::new(size, size)?;
    let ts = tree.size();
    let scale = (size as f32 / ts.width()).min(size as f32 / ts.height());
    let transform = tiny_skia::Transform::from_scale(scale, scale);
    resvg::render(&tree, transform, &mut pixmap.as_mut());
    // tiny_skia is premultiplied RGBA; cairo wants premultiplied BGRA → swap R/B.
    let mut data = pixmap.data().to_vec();
    for px in data.chunks_exact_mut(4) {
        px.swap(0, 2);
    }
    Some((data, size as i32))
}

/// Convert a (pre-validated) inline `image-data` buffer to premultiplied BGRA at
/// `size`×`size`. All bounds were checked in `RawImage::from_wire`, so the
/// indexing here is guaranteed in range.
fn raw_to_bgra(raw: &RawImage, size: u32) -> Option<(Vec<u8>, i32)> {
    let (w, h, ch) = (raw.width(), raw.height(), raw.channels());
    let mut rgba = Vec::with_capacity(w * h * 4);
    for y in 0..h {
        let row = raw.row(y);
        for x in 0..w {
            let i = x * ch;
            rgba.push(row[i]);
            rgba.push(row[i + 1]);
            rgba.push(row[i + 2]);
            rgba.push(if ch == 4 { row[i + 3] } else { 255 });
        }
    }
    let img = image::RgbaImage::from_raw(w as u32, h as u32, rgba)?;
    let resized = image::imageops::resize(&img, size, size, FilterType::Lanczos3);
    let mut data = resized.into_raw();
    premultiply_bgra(&mut data);
    Some((data, size as i32))
}

/// Decode an inline `<img>` source, aspect-preserved to fit `max_w`×`max_h`.
/// Returns premultiplied BGRA + actual `(w, h)`. Local paths / `file://` only.
pub fn load_image(src: &str, max_w: u32, max_h: u32) -> Option<(Vec<u8>, i32, i32)> {
    let path = file_path(src)?;
    let is_svg = matches!(
        path.extension().and_then(|e| e.to_str()),
        Some("svg") | Some("svgz")
    );
    if is_svg {
        let bytes = std::fs::read(&path).ok()?;
        let tree = usvg::Tree::from_data(&bytes, &usvg::Options::default()).ok()?;
        let ts = tree.size();
        let scale = (max_w as f32 / ts.width()).min(max_h as f32 / ts.height());
        let w = ((ts.width() * scale).round() as u32).max(1);
        let h = ((ts.height() * scale).round() as u32).max(1);
        let mut pixmap = tiny_skia::Pixmap::new(w, h)?;
        resvg::render(
            &tree,
            tiny_skia::Transform::from_scale(scale, scale),
            &mut pixmap.as_mut(),
        );
        let mut data = pixmap.data().to_vec();
        for px in data.chunks_exact_mut(4) {
            px.swap(0, 2);
        }
        return Some((data, w as i32, h as i32));
    }
    let img = match image::open(&path) {
        Ok(i) => i,
        Err(e) => {
            debug!(path = %path.display(), error = %e, "inline image decode failed");
            return None;
        }
    };
    let (ow, oh) = (img.width().max(1), img.height().max(1));
    let scale = (max_w as f32 / ow as f32)
        .min(max_h as f32 / oh as f32)
        .min(1.0);
    let w = ((ow as f32 * scale).round() as u32).max(1);
    let h = ((oh as f32 * scale).round() as u32).max(1);
    let mut data = img
        .resize_exact(w, h, FilterType::Lanczos3)
        .to_rgba8()
        .into_raw();
    premultiply_bgra(&mut data);
    Some((data, w as i32, h as i32))
}

/// Treat a string as a filesystem path (stripping `file://`); existing files only.
fn file_path(s: &str) -> Option<PathBuf> {
    let p = s.strip_prefix("file://").unwrap_or(s);
    let pb = PathBuf::from(p);
    pb.is_file().then_some(pb)
}

/// Non-premultiplied RGBA → premultiplied BGRA in place (cairo ARGB32 on LE).
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
