//! Command-line interface. Precedence: defaults < config file < CLI flags.

use std::path::PathBuf;

use clap::Parser;

use crate::config::Config;

#[derive(Debug, Parser)]
#[command(
    name = "gnome-notistack",
    about = "Stacked notification daemon for GNOME Shell 48 / X11",
    version
)]
pub struct Cli {
    /// Render a sample popup and exit (no D-Bus takeover).
    #[arg(long)]
    pub demo_popup: bool,

    /// Path to a config file (overrides the default location).
    #[arg(long, value_name = "FILE")]
    pub config: Option<PathBuf>,

    /// Pango font family for popup text (e.g. "Cantarell", "Sans").
    #[arg(long, value_name = "FAMILY")]
    pub font: Option<String>,

    /// Summary (title) font size, in points.
    #[arg(long, value_name = "PT")]
    pub summary_size: Option<f64>,

    /// Body font size, in points.
    #[arg(long, value_name = "PT")]
    pub body_size: Option<f64>,

    /// Fade in/out duration, in ms (0 disables fading).
    #[arg(long, value_name = "MS")]
    pub fade_ms: Option<u64>,

    /// Popup width, in pixels.
    #[arg(long, value_name = "PX")]
    pub width: Option<u16>,

    /// Default notification timeout, in ms.
    #[arg(long, value_name = "MS")]
    pub default_timeout_ms: Option<u64>,

    /// Max simultaneously displayed popups.
    #[arg(long, value_name = "N")]
    pub max_stack: Option<usize>,
}

impl Cli {
    /// Apply any provided flags on top of a loaded `Config`.
    pub fn apply_to(&self, config: &mut Config) {
        if let Some(v) = &self.font {
            config.font_family = v.clone();
        }
        if let Some(v) = self.summary_size {
            config.summary_size_pt = v;
        }
        if let Some(v) = self.body_size {
            config.body_size_pt = v;
        }
        if let Some(v) = self.fade_ms {
            config.fade_ms = v;
        }
        if let Some(v) = self.width {
            config.width_px = v;
        }
        if let Some(v) = self.default_timeout_ms {
            config.default_timeout_ms = v;
        }
        if let Some(v) = self.max_stack {
            config.max_stack = v;
        }
    }
}
