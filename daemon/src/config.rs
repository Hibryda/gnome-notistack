//! Runtime configuration. Loaded from `$XDG_CONFIG_HOME/gnome-notistack/config.toml`
//! with sensible zero-config defaults (rule 14). All knobs documented here.

use anyhow::Context;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::time::Duration;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
    /// Default expiry for notifications that don't set `expire_timeout` (ms).
    pub default_timeout_ms: u64,
    /// Expiry for low-urgency notifications (ms).
    pub low_urgency_timeout_ms: u64,
    /// Max simultaneously displayed popups; the overflow shows an "N more" card.
    pub max_stack: usize,
    /// Vertical gap between stacked popups (px).
    pub gap_px: u16,
    /// Popup width (px).
    pub width_px: u16,
    /// Screen-edge margin from the top-right anchor (px).
    pub margin_px: u16,
    /// Attempt the `org.gtk.Notifications` takeover (gated by the extension + M0.5 audit).
    /// When false, the daemon runs Fdo-only (partial coverage).
    pub gtk_takeover: bool,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            default_timeout_ms: 5000,
            low_urgency_timeout_ms: 3000,
            max_stack: 5,
            gap_px: 10,
            width_px: 400,
            margin_px: 16,
            gtk_takeover: true,
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

    /// Load config, falling back to defaults when the file is absent (rule 14:
    /// runnable with zero config). A present-but-invalid file is a hard error.
    pub fn load() -> anyhow::Result<Self> {
        match Self::path() {
            Some(p) if p.exists() => {
                let text = std::fs::read_to_string(&p)
                    .with_context(|| format!("reading config {}", p.display()))?;
                toml::from_str(&text).with_context(|| format!("parsing config {}", p.display()))
            }
            _ => Ok(Self::default()),
        }
    }

    fn path() -> Option<PathBuf> {
        let base = std::env::var_os("XDG_CONFIG_HOME")
            .map(PathBuf::from)
            .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".config")))?;
        Some(base.join("gnome-notistack").join("config.toml"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_are_sane() {
        let c = Config::default();
        assert_eq!(c.default_timeout(), Duration::from_millis(5000));
        assert!(c.low_urgency_timeout() < c.default_timeout());
        assert!(c.max_stack >= 1);
    }

    #[test]
    fn parses_partial_toml_over_defaults() {
        let c: Config = toml::from_str("max_stack = 9\ngtk_takeover = false\n").unwrap();
        assert_eq!(c.max_stack, 9);
        assert!(!c.gtk_takeover);
        // Unspecified fields fall back to defaults.
        assert_eq!(c.default_timeout_ms, 5000);
    }
}
