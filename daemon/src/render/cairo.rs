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
    /// Hard cap on the card height (px) — bounds the ARGB surface so no markup
    /// (e.g. a huge font-size) can force an unbounded allocation. Usually the
    /// monitor height.
    pub max_height: i32,
    /// Index (into the returned regions) currently under the pointer, if any —
    /// drawn with a hover highlight. Regions are ordered links-then-buttons.
    pub hover: Option<usize>,
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
/// Vertical padding inside the bottom action bar; horizontal inset for a button
/// label from its segment edges; gap between stacked inline images.
const BAR_PAD_Y: i32 = 11;
const BTN_INSET: i32 = 8;
const IMG_GAP: i32 = 6;

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

    // Action buttons render as a full-width bottom bar (measured here).
    let mut bar_label_h = 0;
    for (_, label) in card.buttons {
        let lm = format!(
            "<span font_family='{font}' size='{bs}' foreground='{btn_hex}'>{label}</span>",
            bs = pango_size(card.body_pt),
            label = crate::markup::escape(label),
        );
        bar_label_h = bar_label_h.max(make_layout(&lm).pixel_size().1);
    }
    let bar_h = if card.buttons.is_empty() {
        0
    } else {
        bar_label_h + 2 * BAR_PAD_Y
    };

    // Content area (icon + title + body + images), then the bar below it.
    let mut content_h = title_h;
    if has_body {
        content_h += gap + body_h;
    }
    if images_h > 0 {
        content_h += gap + images_h;
    }
    let content_area = (content_h.max(icon_size) + 2 * PAD).max(MIN_HEIGHT);
    // Clamp to bound the surface allocation (DoS defense: untrusted markup can
    // request enormous font sizes → enormous content height). Tall content is
    // simply cropped.
    let height = (content_area + bar_h).clamp(MIN_HEIGHT, card.max_height.max(MIN_HEIGHT));

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

        // Body + link regions. Link rects are computed before the text is drawn
        // so the hovered link's highlight can sit behind it.
        if has_body {
            y += gap;
            let bl = make_on(&cr, &body_markup, text_width);
            let text = bl.text();
            let mut link_rects: Vec<(i32, i32, i32, i32, String)> = Vec::new();
            // Advance a cursor so repeated link texts map to successive occurrences
            // (not all to the first match).
            let mut search_from = 0usize;
            for (url, ltext) in card.links {
                if ltext.is_empty() {
                    continue;
                }
                if let Some(rel) = text
                    .as_str()
                    .get(search_from..)
                    .and_then(|t| t.find(ltext.as_str()))
                {
                    let byte = search_from + rel;
                    search_from = byte + ltext.len();
                    let sp = bl.index_to_pos(byte as i32);
                    let ep = bl.index_to_pos((byte + ltext.len()) as i32);
                    let s = ::pango::SCALE;
                    let (sx, sy, sh) = (sp.x() / s, sp.y() / s, sp.height() / s);
                    let (ex, ey) = (ep.x() / s, ep.y() / s);
                    let r = if sy == ey {
                        (text_x + sx.min(ex), y + sy, (ex - sx).abs(), sh)
                    } else {
                        (text_x, y + sy, text_w_px, (ey + sh) - sy)
                    };
                    link_rects.push((r.0, r.1, r.2.max(1), r.3.max(1), url.clone()));
                }
            }
            // Hover highlight (link regions are indices 0..link_rects.len()).
            for (i, (lx, ly, lw, lh, _)) in link_rects.iter().enumerate() {
                if card.hover == Some(i) {
                    rounded_rect(
                        &cr,
                        (*lx - 3) as f64,
                        (*ly - 1) as f64,
                        (*lw + 6) as f64,
                        (*lh + 2) as f64,
                        4.0,
                    );
                    cr.set_source_rgba(card.fg[0], card.fg[1], card.fg[2], 0.20);
                    cr.fill().ok();
                }
            }
            cr.move_to(text_x as f64, y as f64);
            cr.set_source_rgba(card.fg[0], card.fg[1], card.fg[2], card.fg[3]);
            ::pangocairo::functions::show_layout(&cr, &bl);
            for (lx, ly, lw, lh, url) in link_rects {
                regions.push(Region {
                    x: lx,
                    y: ly,
                    w: lw,
                    h: lh,
                    kind: RegionKind::Link(url),
                });
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

        // Action bar: a full-width bottom strip of equal segments separated by
        // hairlines, clipped to the card so the bottom corners stay rounded.
        if bar_h > 0 {
            let bar_top = content_area;
            let n = card.buttons.len() as i32;
            cr.save().ok();
            rounded_rect(&cr, 0.5, 0.5, w - 1.0, h - 1.0, r);
            cr.clip();
            // Subtle bar fill + top separator.
            cr.set_source_rgba(card.fg[0], card.fg[1], card.fg[2], 0.05);
            cr.rectangle(0.0, bar_top as f64, w, bar_h as f64);
            cr.fill().ok();
            cr.set_source_rgba(card.fg[0], card.fg[1], card.fg[2], 0.14);
            cr.set_line_width(1.0);
            cr.move_to(0.0, bar_top as f64 + 0.5);
            cr.line_to(w, bar_top as f64 + 0.5);
            cr.stroke().ok();

            let n_links = regions.len();
            let seg = card.width / n;
            for (i, (key, label)) in card.buttons.iter().enumerate() {
                let ii = i as i32;
                let x0 = ii * seg;
                let seg_w = if ii == n - 1 { card.width - x0 } else { seg };
                // Hover highlight for this segment (over the base bar fill).
                if card.hover == Some(n_links + i) {
                    cr.set_source_rgba(card.fg[0], card.fg[1], card.fg[2], 0.10);
                    cr.rectangle(x0 as f64, bar_top as f64, seg_w as f64, bar_h as f64);
                    cr.fill().ok();
                }
                cr.set_source_rgba(card.fg[0], card.fg[1], card.fg[2], 0.14);
                if ii > 0 {
                    cr.move_to(x0 as f64 + 0.5, bar_top as f64);
                    cr.line_to(x0 as f64 + 0.5, height as f64);
                    cr.stroke().ok();
                }
                // Centered, ellipsized label within the segment.
                let lm = format!(
                    "<span font_family='{font}' size='{bs}' foreground='{btn_hex}'>{label}</span>",
                    bs = pango_size(card.body_pt),
                    label = crate::markup::escape(label),
                );
                let ll = ::pangocairo::functions::create_layout(&cr);
                ll.set_markup(&lm);
                ll.set_width(((seg_w - 2 * BTN_INSET).max(1)) * ::pango::SCALE);
                ll.set_alignment(::pango::Alignment::Center);
                ll.set_ellipsize(::pango::EllipsizeMode::End);
                let lh = ll.pixel_size().1;
                cr.move_to((x0 + BTN_INSET) as f64, (bar_top + (bar_h - lh) / 2) as f64);
                cr.set_source_rgba(card.fg[0], card.fg[1], card.fg[2], card.fg[3]);
                ::pangocairo::functions::show_layout(&cr, &ll);
                regions.push(Region {
                    x: x0,
                    y: bar_top,
                    w: seg_w,
                    h: bar_h,
                    kind: RegionKind::Button(key.clone()),
                });
            }
            cr.restore().ok();
        }
        // Individual draw ops above use `.ok()` as best-effort, but a cairo
        // context is sticky: once it enters an error status every later op
        // silently no-ops, yielding a blank/partial card. Surface the poisoned
        // state instead of returning a silently-broken buffer (rule 02).
        cr.status()
            .context("cairo context error during card draw")?;
    }

    surface.flush();
    surface
        .status()
        .context("cairo surface error after card draw")?;
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
