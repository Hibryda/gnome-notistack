//! Cairo/pango card drawing.
//!
//! Draws onto an `ImageSurface` (ARGB32) and hands the premultiplied BGRA buffer
//! to `render::x11::Ui::put_argb` (the safe path; the unsafe XCBSurface fast path
//! is a deferred optimization — see docs/known-loss.md).
//!
//! The card is laid out top-to-bottom: title, (gap) body — with clickable link
//! rects — (gap) inline images, (gap) action buttons. Height is content-derived.
//! `render_card` returns the interactive [`Region`]s for click hit-testing.

use anyhow::{Context, Result};

/// What a clickable region does when hit.
#[derive(Clone, Debug)]
pub enum RegionKind {
    /// An action button: the action key (Fdo) or `app.`-name (GTK) to invoke.
    Button(String),
    /// A hyperlink: the URL to open.
    Link(String),
}

/// A clickable rectangle within a popup, in window-local pixels.
#[derive(Clone, Debug)]
pub struct Region {
    pub x: i32,
    pub y: i32,
    pub w: i32,
    pub h: i32,
    pub kind: RegionKind,
}

impl Region {
    pub fn contains(&self, px: i32, py: i32) -> bool {
        px >= self.x && px < self.x + self.w && py >= self.y && py < self.y + self.h
    }
}

/// Content to draw on a popup card. Height is derived from the content.
pub struct Card<'a> {
    pub summary: &'a str,
    pub body: &'a str,
    pub width: i32,
    /// Optional icon: premultiplied BGRA (cairo ARGB32 order) and its size in px.
    pub icon: Option<(Vec<u8>, i32)>,
    /// Inline body images: premultiplied BGRA + `(w, h)` px (already decoded).
    pub inline_images: &'a [(Vec<u8>, i32, i32)],
    /// Action buttons as `(action_key, label)` (the whole-card "default" excluded).
    pub buttons: &'a [(String, String)],
    /// Body hyperlinks as `(url, visible_text)` for click hit-testing.
    pub links: &'a [(String, String)],
    /// Pango font family.
    pub font: &'a str,
    /// Summary / body font sizes, in points.
    pub summary_pt: f64,
    pub body_pt: f64,
    /// Gap between the title and the body, in px.
    pub title_body_gap: i32,
    /// Background / foreground colors (RGBA 0–1), from the theme or overrides.
    pub bg: [f64; 4],
    pub fg: [f64; 4],
}

/// Pango `size` attribute is in 1024ths of a point.
fn pango_size(pt: f64) -> i32 {
    (pt * 1024.0).round() as i32
}

/// `#rrggbb` from an RGBA color (alpha ignored — Pango foreground is opaque).
fn rgb_hex(c: [f64; 4]) -> String {
    let q = |v: f64| (v.clamp(0.0, 1.0) * 255.0).round() as u8;
    format!("#{:02x}{:02x}{:02x}", q(c[0]), q(c[1]), q(c[2]))
}

/// Blend `a` toward `b` by `t` (0=a, 1=b) — used to dim the body text.
fn mix(a: [f64; 4], b: [f64; 4], t: f64) -> [f64; 4] {
    [
        a[0] + (b[0] - a[0]) * t,
        a[1] + (b[1] - a[1]) * t,
        a[2] + (b[2] - a[2]) * t,
        a[3],
    ]
}

/// Inner padding around the card content, in pixels.
const PAD: i32 = 14;
/// Minimum card height (so a one-line notification still looks like a card).
const MIN_HEIGHT: i32 = 44;
/// Button padding (x, y) and inter-button gap; gap between stacked inline images.
const BTN_PAD_X: i32 = 12;
const BTN_PAD_Y: i32 = 6;
const BTN_GAP: i32 = 8;
const IMG_GAP: i32 = 6;

/// A placed button (relative to the button area origin) for measure→draw reuse.
struct PlacedBtn {
    key: String,
    label: String,
    rx: i32,
    ry: i32,
    w: i32,
    h: i32,
}

/// A rendered card: premultiplied BGRA (cairo `ARgb32`), the row `stride`, the
/// chosen `height`, and the clickable regions (buttons + links).
pub fn render_card(card: &Card) -> Result<(Vec<u8>, i32, i32, Vec<Region>)> {
    let font = crate::markup::escape(card.font);
    let title_hex = rgb_hex(card.fg);
    let body_hex = rgb_hex(mix(card.fg, card.bg, 0.28));
    let btn_hex = title_hex.clone();
    let title_markup = format!(
        "<span font_family='{font}' weight='bold' size='{ss}' foreground='{title_hex}'>{summary}</span>",
        ss = pango_size(card.summary_pt),
        summary = crate::markup::escape(card.summary),
    );
    let has_body = !card.body.trim().is_empty();
    let body_markup = format!(
        "<span font_family='{font}' size='{bs}' foreground='{body_hex}'>{body}</span>",
        bs = pango_size(card.body_pt),
        body = crate::markup::to_pango(card.body),
    );

    let icon_size = card.icon.as_ref().map(|(_, s)| *s).unwrap_or(0);
    let text_x = PAD + if icon_size > 0 { icon_size + PAD } else { 0 };
    let text_w_px = (card.width - text_x - PAD).max(1);
    let text_width = text_w_px * ::pango::SCALE;
    let gap = card.title_body_gap;

    // --- Measure pass (throwaway surface) ---
    let measure = ::cairo::ImageSurface::create(::cairo::Format::ARgb32, card.width, 1)
        .context("create measuring surface")?;
    let mcr = ::cairo::Context::new(&measure).context("create measuring context")?;

    let make_layout = |markup: &str| {
        let l = ::pangocairo::functions::create_layout(&mcr);
        l.set_markup(markup);
        l.set_width(text_width);
        l.set_wrap(::pango::WrapMode::WordChar);
        l
    };
    let title_h = make_layout(&title_markup).pixel_size().1;
    let body_h = if has_body {
        make_layout(&body_markup).pixel_size().1
    } else {
        0
    };

    // Inline images, scaled to fit the text column width.
    let mut img_draw: Vec<(&[u8], i32, i32, i32, i32)> = Vec::new(); // data, srcw, srch, dw, dh
    let mut images_h = 0;
    for (data, w, h) in card.inline_images {
        let dw = (*w).min(text_w_px).max(1);
        let dh = if *w > 0 { h * dw / *w } else { *h };
        if !img_draw.is_empty() {
            images_h += IMG_GAP;
        }
        images_h += dh;
        img_draw.push((data, *w, *h, dw, dh));
    }

    // Action buttons: measure, then pack into rows within the text column.
    let btn_area_w = text_w_px;
    let mut placed: Vec<PlacedBtn> = Vec::new();
    let (mut bx, mut by, mut row_h) = (0, 0, 0);
    for (key, label) in card.buttons {
        let lm = format!(
            "<span font_family='{font}' size='{bs}' foreground='{btn_hex}'>{label}</span>",
            bs = pango_size(card.body_pt),
            label = crate::markup::escape(label),
        );
        let (lw, lh) = make_layout(&lm).pixel_size();
        let (w, h) = (lw + 2 * BTN_PAD_X, lh + 2 * BTN_PAD_Y);
        if bx > 0 && bx + w > btn_area_w {
            by += row_h + BTN_GAP;
            bx = 0;
            row_h = 0;
        }
        placed.push(PlacedBtn {
            key: key.clone(),
            label: label.clone(),
            rx: bx,
            ry: by,
            w,
            h,
        });
        bx += w + BTN_GAP;
        row_h = row_h.max(h);
    }
    let buttons_h = if placed.is_empty() { 0 } else { by + row_h };

    let mut content_h = title_h;
    if has_body {
        content_h += gap + body_h;
    }
    if images_h > 0 {
        content_h += gap + images_h;
    }
    if buttons_h > 0 {
        content_h += gap + buttons_h;
    }
    let height = (content_h.max(icon_size) + 2 * PAD).max(MIN_HEIGHT);

    // --- Draw pass ---
    let mut surface = ::cairo::ImageSurface::create(::cairo::Format::ARgb32, card.width, height)
        .context("create cairo image surface")?;
    let mut regions: Vec<Region> = Vec::new();
    {
        let cr = ::cairo::Context::new(&surface).context("create cairo context")?;
        cr.set_operator(::cairo::Operator::Source);
        cr.set_source_rgba(0.0, 0.0, 0.0, 0.0);
        cr.paint().ok();
        cr.set_operator(::cairo::Operator::Over);

        // Rounded translucent background (theme color) + subtle border.
        let (w, h, r) = (card.width as f64, height as f64, 12.0);
        rounded_rect(&cr, 0.5, 0.5, w - 1.0, h - 1.0, r);
        cr.set_source_rgba(card.bg[0], card.bg[1], card.bg[2], card.bg[3]);
        cr.fill_preserve().ok();
        cr.set_source_rgba(card.fg[0], card.fg[1], card.fg[2], 0.10);
        cr.set_line_width(1.0);
        cr.stroke().ok();

        // Icon (top-aligned with the title).
        if let Some((data, isize)) = &card.icon {
            if let Ok(icon) = ::cairo::ImageSurface::create_for_data(
                data.clone(),
                ::cairo::Format::ARgb32,
                *isize,
                *isize,
                *isize * 4,
            ) {
                if cr.set_source_surface(&icon, PAD as f64, PAD as f64).is_ok() {
                    cr.paint().ok();
                }
            }
        }

        let mut y = PAD;
        // Title.
        let tl = make_on(&cr, &title_markup, text_width);
        cr.move_to(text_x as f64, y as f64);
        cr.set_source_rgba(card.fg[0], card.fg[1], card.fg[2], card.fg[3]);
        ::pangocairo::functions::show_layout(&cr, &tl);
        y += title_h;

        // Body + link regions.
        if has_body {
            y += gap;
            let bl = make_on(&cr, &body_markup, text_width);
            cr.move_to(text_x as f64, y as f64);
            ::pangocairo::functions::show_layout(&cr, &bl);
            let text = bl.text();
            for (url, ltext) in card.links {
                if ltext.is_empty() {
                    continue;
                }
                if let Some(byte) = text.as_str().find(ltext.as_str()) {
                    let sp = bl.index_to_pos(byte as i32);
                    let ep = bl.index_to_pos((byte + ltext.len()) as i32);
                    let s = ::pango::SCALE;
                    let (sx, sy, sh) = (sp.x() / s, sp.y() / s, sp.height() / s);
                    let (ex, ey) = (ep.x() / s, ep.y() / s);
                    let rect = if sy == ey {
                        (text_x + sx.min(ex), y + sy, (ex - sx).abs(), sh)
                    } else {
                        (text_x, y + sy, text_w_px, (ey + sh) - sy)
                    };
                    regions.push(Region {
                        x: rect.0,
                        y: rect.1,
                        w: rect.2.max(1),
                        h: rect.3.max(1),
                        kind: RegionKind::Link(url.clone()),
                    });
                }
            }
            y += body_h;
        }

        // Inline images (stacked).
        if images_h > 0 {
            y += gap;
            for (i, (data, sw, sh, dw, dh)) in img_draw.iter().enumerate() {
                if i > 0 {
                    y += IMG_GAP;
                }
                if let Ok(img) = ::cairo::ImageSurface::create_for_data(
                    data.to_vec(),
                    ::cairo::Format::ARgb32,
                    *sw,
                    *sh,
                    *sw * 4,
                ) {
                    cr.save().ok();
                    cr.translate(text_x as f64, y as f64);
                    cr.scale(*dw as f64 / *sw as f64, *dh as f64 / *sh as f64);
                    if cr.set_source_surface(&img, 0.0, 0.0).is_ok() {
                        cr.paint().ok();
                    }
                    cr.restore().ok();
                }
                y += dh;
            }
        }

        // Action buttons.
        if buttons_h > 0 {
            y += gap;
            for b in &placed {
                let (ax, ay) = (text_x + b.rx, y + b.ry);
                rounded_rect(
                    &cr,
                    ax as f64 + 0.5,
                    ay as f64 + 0.5,
                    (b.w - 1) as f64,
                    (b.h - 1) as f64,
                    6.0,
                );
                cr.set_source_rgba(card.fg[0], card.fg[1], card.fg[2], 0.08);
                cr.fill_preserve().ok();
                cr.set_source_rgba(card.fg[0], card.fg[1], card.fg[2], 0.35);
                cr.set_line_width(1.0);
                cr.stroke().ok();
                let lm = format!(
                    "<span font_family='{font}' size='{bs}' foreground='{btn_hex}'>{label}</span>",
                    bs = pango_size(card.body_pt),
                    label = crate::markup::escape(&b.label),
                );
                let ll = make_on(&cr, &lm, -1);
                cr.move_to((ax + BTN_PAD_X) as f64, (ay + BTN_PAD_Y) as f64);
                ::pangocairo::functions::show_layout(&cr, &ll);
                regions.push(Region {
                    x: ax,
                    y: ay,
                    w: b.w,
                    h: b.h,
                    kind: RegionKind::Button(b.key.clone()),
                });
            }
        }
    }

    surface.flush();
    let stride = surface.stride();
    let data = surface.data().context("borrow cairo surface data")?;
    Ok((data.to_vec(), stride, height, regions))
}

/// Create a wrapped Pango layout on a context; `width = -1` disables wrapping.
fn make_on(cr: &::cairo::Context, markup: &str, width: i32) -> ::pango::Layout {
    let l = ::pangocairo::functions::create_layout(cr);
    l.set_markup(markup);
    if width > 0 {
        l.set_width(width);
        l.set_wrap(::pango::WrapMode::WordChar);
    }
    l
}

fn rounded_rect(cr: &::cairo::Context, x: f64, y: f64, w: f64, h: f64, r: f64) {
    use std::f64::consts::PI;
    cr.new_sub_path();
    cr.arc(x + w - r, y + r, r, -0.5 * PI, 0.0);
    cr.arc(x + w - r, y + h - r, r, 0.0, 0.5 * PI);
    cr.arc(x + r, y + h - r, r, 0.5 * PI, PI);
    cr.arc(x + r, y + r, r, PI, 1.5 * PI);
    cr.close_path();
}
