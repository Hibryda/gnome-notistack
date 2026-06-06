//! Command-line interface. Configuration lives in GSettings (schema
//! `org.gnome.shell.extensions.notistack`), edited via the extension's
//! preferences window — see `config.rs`. The CLI only carries run-mode flags.

use clap::Parser;

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
}
