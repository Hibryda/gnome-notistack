//! Cairo/pango card drawing.
//!
//! M2 implements the **safe path**: draw onto an `ImageSurface` (ARGB32) and
//! hand the premultiplied BGRA buffer to `render::x11::Ui::put_argb`. The
//! load-bearing **unsafe XCBSurface seam** (cairo against x11rb's raw XCB
//! pointer — the faster path, plan risk R5) is a follow-up optimization gated by
//! `render::window::probe_render_mode`; the fallback implemented here is always
//! correct ("naive then optimize", rule 06).
//!
//! M4 sizes the card to its content: the text is laid out once to measure the
//! required height, then drawn.

use anyhow::{Context, Result};

/// Content to draw on a popup card. Height is derived from the content.
pub struct Card<'a> {
    pub summary: &'a str,
    pub body: &'a str,
    pub width: i32,
    /// Optional icon: premultiplied BGRA (cairo ARGB32 order) and its size in px.
    pub icon: Option<(Vec<u8>, i32)>,
    /// Pango font family.
    pub font: &'a str,
    /// Summary / body font sizes, in points.
    pub summary_pt: f64,
    pub body_pt: f64,
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

/// A rendered card: premultiplied BGRA (cairo `ARgb32`), the row `stride`
/// (may exceed `width * 4` due to cairo padding), and the chosen `height`.
pub fn render_card(card: &Card) -> Result<(Vec<u8>, i32, i32)> {
    // Summary is plain text (escaped); body is FDO markup → Pango markup.
    // Summary uses the foreground color; body is dimmed toward the background.
    let font = crate::markup::escape(card.font);
    let summary_hex = rgb_hex(card.fg);
    let body_hex = rgb_hex(mix(card.fg, card.bg, 0.28));
    let markup = format!(
        "<span font_family='{font}' weight='bold' size='{ss}' foreground='{summary_hex}'>{summary}</span>\n\
         <span font_family='{font}' size='{bs}' foreground='{body_hex}'>{body}</span>",
        ss = pango_size(card.summary_pt),
        bs = pango_size(card.body_pt),
        summary = crate::markup::escape(card.summary),
        body = crate::markup::to_pango(card.body),
    );
    let icon_size = card.icon.as_ref().map(|(_, s)| *s).unwrap_or(0);
    let text_x = PAD + if icon_size > 0 { icon_size + PAD } else { 0 };
    let text_width = (card.width - text_x - PAD) * ::pango::SCALE;

    // Measure pass: lay out the text on a throwaway surface to get its height.
    let height = {
        let measure = ::cairo::ImageSurface::create(::cairo::Format::ARgb32, card.width, 1)
            .context("create measuring surface")?;
        let cr = ::cairo::Context::new(&measure).context("create measuring context")?;
        let layout = ::pangocairo::functions::create_layout(&cr);
        layout.set_markup(&markup);
        layout.set_width(text_width);
        layout.set_wrap(::pango::WrapMode::WordChar);
        let (_, text_h) = layout.pixel_size();
        (text_h.max(icon_size) + 2 * PAD).max(MIN_HEIGHT)
    };

    let mut surface = ::cairo::ImageSurface::create(::cairo::Format::ARgb32, card.width, height)
        .context("create cairo image surface")?;
    {
        let cr = ::cairo::Context::new(&surface).context("create cairo context")?;

        // Start fully transparent so the rounded corners read as alpha=0.
        cr.set_operator(::cairo::Operator::Source);
        cr.set_source_rgba(0.0, 0.0, 0.0, 0.0);
        cr.paint().ok();
        cr.set_operator(::cairo::Operator::Over);

        // Rounded translucent background (theme color).
        let (w, h, r) = (card.width as f64, height as f64, 12.0);
        rounded_rect(&cr, 0.5, 0.5, w - 1.0, h - 1.0, r);
        cr.set_source_rgba(card.bg[0], card.bg[1], card.bg[2], card.bg[3]);
        cr.fill_preserve().ok();
        // Subtle border from the foreground color.
        cr.set_source_rgba(card.fg[0], card.fg[1], card.fg[2], 0.10);
        cr.set_line_width(1.0);
        cr.stroke().ok();

        // Icon (vertically centered on the left), if any.
        if let Some((data, isize)) = &card.icon {
            let stride = isize * 4; // ARGB32 stride == width*4
            if let Ok(icon) = ::cairo::ImageSurface::create_for_data(
                data.clone(),
                ::cairo::Format::ARgb32,
                *isize,
                *isize,
                stride,
            ) {
                let iy = ((height - isize) / 2) as f64;
                if cr.set_source_surface(&icon, PAD as f64, iy).is_ok() {
                    cr.paint().ok();
                }
            }
        }

        // Text.
        let layout = ::pangocairo::functions::create_layout(&cr);
        layout.set_markup(&markup);
        layout.set_width(text_width);
        layout.set_wrap(::pango::WrapMode::WordChar);
        cr.move_to(text_x as f64, PAD as f64);
        cr.set_source_rgba(card.fg[0], card.fg[1], card.fg[2], card.fg[3]);
        ::pangocairo::functions::show_layout(&cr, &layout);
    }

    surface.flush();
    let stride = surface.stride();
    let data = surface.data().context("borrow cairo surface data")?;
    Ok((data.to_vec(), stride, height))
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
