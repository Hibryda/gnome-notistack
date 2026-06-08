//! Runtime configuration, backed by **GSettings** (schema
//! `org.gnome.shell.extensions.notistack`, shared with the extension's prefs
//! window). Read at startup and re-read live by the render thread. Font and
//! colors fall back to the **system theme** (`org.gnome.desktop.interface`
//! `font-name` / `color-scheme`, Adwaita palette) unless explicitly overridden.
//!
//! If the schema isn't installed (e.g. `GSETTINGS_SCHEMA_DIR` unset), the daemon
//! still runs on built-in defaults (rule 14: runnable with zero config).

use std::time::Duration;

use gio::prelude::*;
use tracing::warn;

pub const SCHEMA_ID: &str = "org.gnome.shell.extensions.notistack";
const IFACE_SCHEMA: &str = "org.gnome.desktop.interface";

/// RGBA in 0.0–1.0.
pub type Rgba = [f64; 4];

// Adwaita window background / foreground (light + dark), alpha for translucency.
const ADW_DARK_BG: Rgba = [0.141, 0.141, 0.141, 0.96]; // #242424
const ADW_DARK_FG: Rgba = [1.0, 1.0, 1.0, 1.0];
const ADW_LIGHT_BG: Rgba = [0.980, 0.980, 0.984, 0.97]; // #fafafb
const ADW_LIGHT_FG: Rgba = [0.180, 0.204, 0.212, 1.0]; // #2e3436

#[derive(Debug, Clone, PartialEq)]
pub struct Config {
    pub default_timeout_ms: u64,
    pub low_urgency_timeout_ms: u64,
    /// Floor on display time (ms): if > 0, a finite expiry is raised so the popup
    /// shows for at least this long. 0 = no floor. (A never-expire popup already
    /// satisfies any floor.)
    pub min_timeout_ms: u64,
    /// Hard ceiling on display time (ms): if > 0, every popup is force-closed no
    /// later than this from creation, overriding longer/never-expire timeouts.
    /// 0 = no cap (respect each notification's own timeout). When the floor is
    /// >= this ceiling, display time collapses to a constant = the ceiling.
    pub max_timeout_ms: u64,
    pub max_stack: usize,
    pub gap_px: u16,
    pub width_px: u16,
    pub width_height_fraction: f64,
    pub max_width_fraction: f64,
    pub margin_px: u16,
    /// Monitor to render on: "primary" or a RandR connector name (e.g. "DP-1").
    pub monitor: String,
    /// Active icon theme (from the system), used to resolve themed app icons.
    pub icon_theme: String,
    pub font_family: String,
    pub summary_size_pt: f64,
    pub body_size_pt: f64,
    /// Vertical gap between the title and the body, in px.
    pub title_body_gap_px: u16,
    pub fade_ms: u64,
    pub history_size: usize,
    pub suppress_on_fullscreen: bool,
    pub gtk_takeover: bool,
    /// Resolved card background / foreground colors (from theme unless overridden).
    pub bg: Rgba,
    pub fg: Rgba,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            default_timeout_ms: 5000,
            low_urgency_timeout_ms: 3000,
            min_timeout_ms: 0,
            max_timeout_ms: 0,
            max_stack: 5,
            gap_px: 10,
            width_px: 0,
            width_height_fraction: 0.30,
            max_width_fraction: 0.18,
            margin_px: 16,
            monitor: "primary".to_string(),
            icon_theme: "hicolor".to_string(),
            font_family: "Sans".to_string(),
            summary_size_pt: 12.0,
            body_size_pt: 10.0,
            title_body_gap_px: 4,
            fade_ms: 150,
            history_size: 100,
            suppress_on_fullscreen: true,
            gtk_takeover: true,
            bg: ADW_DARK_BG,
            fg: ADW_DARK_FG,
        }
    }
}

impl Config {
    pub fn default_timeout(&self) -> Duration {
        Duration::from_millis(self.default_timeout_ms)
    }

    pub fn low_urgency_timeout(&self) -> Duration {
        Duration::from_millis(self.low_urgency_timeout_ms)
    }

    /// Load config from GSettings, falling back to defaults if the schema is absent.
    pub fn load() -> Config {
        match open() {
            Some((ours, iface)) => Config::from_settings(&ours, iface.as_ref()),
            None => {
                warn!(
                    schema = SCHEMA_ID,
                    "gschema not found; using defaults (set GSETTINGS_SCHEMA_DIR)"
                );
                Config::default()
            }
        }
    }

    /// Build a Config from open Settings handles (used for the live re-read too).
    // Start from defaults, then map each key + resolve font/colors with logic.
    #[allow(clippy::field_reassign_with_default)]
    pub fn from_settings(ours: &gio::Settings, iface: Option<&gio::Settings>) -> Config {
        let mut c = Config::default();
        c.default_timeout_ms = ours.uint("default-timeout-ms") as u64;
        c.low_urgency_timeout_ms = ours.uint("low-urgency-timeout-ms") as u64;
        c.min_timeout_ms = ours.uint("min-timeout-ms") as u64;
        c.max_timeout_ms = ours.uint("max-timeout-ms") as u64;
        c.max_stack = (ours.uint("max-stack") as usize).max(1);
        c.gap_px = ours.uint("gap-px") as u16;
        c.width_px = ours.uint("width-px") as u16;
        c.width_height_fraction = ours.double("width-height-fraction");
        c.max_width_fraction = ours.double("max-width-fraction");
        c.margin_px = ours.uint("margin-px") as u16;
        c.monitor = ours.string("monitor").to_string();
        c.title_body_gap_px = ours.uint("title-body-gap-px") as u16;
        c.fade_ms = ours.uint("fade-ms") as u64;
        c.history_size = ours.uint("history-size") as usize;
        c.suppress_on_fullscreen = ours.boolean("suppress-on-fullscreen");
        c.gtk_takeover = ours.boolean("gtk-takeover");

        // Active icon theme (for themed app-icon resolution).
        if let Some(theme) = iface.map(|i| i.string("icon-theme")) {
            if !theme.is_empty() {
                c.icon_theme = theme.to_string();
            }
        }

        // Font: override or system interface font.
        let (sys_family, sys_size) = iface
            .map(|i| parse_font(&i.string("font-name")))
            .unwrap_or_else(|| ("Sans".to_string(), 11.0));
        let ff = ours.string("font-family");
        c.font_family = if ff.is_empty() {
            sys_family
        } else {
            ff.to_string()
        };
        let ss = ours.double("summary-size-pt");
        c.summary_size_pt = if ss > 0.0 {
            ss
        } else {
            (sys_size * 1.15).round()
        };
        let bs = ours.double("body-size-pt");
        c.body_size_pt = if bs > 0.0 { bs } else { sys_size };

        // Colors: theme (light/dark via color-scheme) unless overridden.
        let dark = match ours.string("theme-mode").as_str() {
            "dark" => true,
            "light" => false,
            _ => iface
                .map(|i| i.string("color-scheme") == "prefer-dark")
                .unwrap_or(false),
        };
        c.bg = if dark { ADW_DARK_BG } else { ADW_LIGHT_BG };
        c.fg = if dark { ADW_DARK_FG } else { ADW_LIGHT_FG };
        if let Some(bg) = parse_color(&ours.string("background-color")) {
            c.bg = bg;
        }
        if let Some(fg) = parse_color(&ours.string("foreground-color")) {
            c.fg = fg;
        }
        c
    }
}

/// Open our schema (+ the interface schema for theme defaults), or None if ours
/// isn't installed. `gio::Settings::new` aborts on a missing schema, so we check
/// the source first.
pub fn open() -> Option<(gio::Settings, Option<gio::Settings>)> {
    let source = gio::SettingsSchemaSource::default()?;
    source.lookup(SCHEMA_ID, true)?;
    let ours = gio::Settings::new(SCHEMA_ID);
    let iface = source
        .lookup(IFACE_SCHEMA, true)
        .map(|_| gio::Settings::new(IFACE_SCHEMA));
    Some((ours, iface))
}

/// Parse a Pango/GNOME `font-name` ("Cantarell 11", "Source Sans 3 Bold 12") into
/// (family, size_pt). The trailing numeric token is the size; the rest is family.
fn parse_font(s: &str) -> (String, f64) {
    let s = s.trim();
    if let Some((rest, last)) = s.rsplit_once(' ') {
        if let Ok(size) = last.parse::<f64>() {
            if !rest.trim().is_empty() {
                return (rest.trim().to_string(), size);
            }
        }
    }
    (
        if s.is_empty() {
            "Sans".into()
        } else {
            s.into()
        },
        11.0,
    )
}

/// Parse `#rrggbb` or `#rrggbbaa` into RGBA (0–1). Returns None for empty/invalid.
fn parse_color(s: &str) -> Option<Rgba> {
    let h = s.trim().strip_prefix('#')?;
    // Guard byte-slicing below against multibyte chars: a value like "#a€00" is
    // 6 bytes but slicing `h[0..2]` would split the `€` and panic. Hex is ASCII.
    if !h.bytes().all(|b| b.is_ascii_hexdigit()) {
        return None;
    }
    let byte = |i: usize| {
        u8::from_str_radix(&h[i..i + 2], 16)
            .ok()
            .map(|v| v as f64 / 255.0)
    };
    match h.len() {
        6 => Some([byte(0)?, byte(2)?, byte(4)?, 1.0]),
        8 => Some([byte(0)?, byte(2)?, byte(4)?, byte(6)?]),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_font_name() {
        assert_eq!(parse_font("Cantarell 11"), ("Cantarell".into(), 11.0));
        assert_eq!(
            parse_font("Source Sans 3 Bold 12"),
            ("Source Sans 3 Bold".into(), 12.0)
        );
        assert_eq!(parse_font("Sans").0, "Sans");
    }

    #[test]
    fn parses_colors() {
        assert_eq!(parse_color("#ff0000"), Some([1.0, 0.0, 0.0, 1.0]));
        assert_eq!(parse_color("#00000080").unwrap()[3], 128.0 / 255.0);
        assert_eq!(parse_color(""), None);
        assert_eq!(parse_color("nope"), None);
    }

    #[test]
    fn parse_color_rejects_non_ascii_without_panicking() {
        // Multibyte char in a 6-byte string must not panic on a non-boundary slice.
        assert_eq!(parse_color("#a\u{20ac}00"), None); // "#a€00"
        assert_eq!(parse_color("#zz0000"), None);
        assert_eq!(parse_color("#12"), None);
    }
}
