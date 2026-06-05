//! gnome-notistack — standalone FreeDesktop notification daemon for GNOME Shell 48 / X11.
//!
//! Shows multiple notifications at once as a vertical stack of override-redirect
//! ARGB popups (dunst-style, on GNOME). See `docs/IMPLEMENTATION-PLAN.md`.
//!
//! This is the **M0 scaffold**: every module is wired and the crate compiles;
//! `// M<n>:` comments mark where each milestone's behavior lands.
#![allow(dead_code)] // M0 scaffold: stubs are not yet wired into the run loop.

mod a11y;
mod bus;
mod config;
mod dbus;
mod error;
mod event_loop;
mod history;
mod lockscreen;
mod markup;
mod name_watcher;
mod notification;
mod render;
mod shutdown;
mod sound;
mod stack;

use anyhow::Context;
use tracing::info;

fn main() -> anyhow::Result<()> {
    init_tracing();

    // Standalone visual smoke test for M2 rendering (no D-Bus takeover needed).
    if std::env::args().any(|a| a == "--demo-popup") {
        return render::demo();
    }

    let config = config::Config::load().context("loading configuration")?;
    info!(?config, "gnome-notistack starting");

    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .context("building tokio runtime")?;

    runtime.block_on(run(config))
}

async fn run(config: config::Config) -> anyhow::Result<()> {
    // Serve the D-Bus interfaces and queue for the notification name(s). The
    // companion extension frees the names; D-Bus then promotes us to owner.
    let _conn = bus::serve(&config).await?;
    info!("serving D-Bus; queued for the notification name(s) awaiting release");

    // M2+: spawn the X11 event loop and drive the popup stack on name promotion.
    shutdown::wait_for_shutdown().await;
    info!("gnome-notistack shutting down");
    Ok(())
}

fn init_tracing() {
    use tracing_subscriber::prelude::*;
    use tracing_subscriber::EnvFilter;

    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));

    // 12-Factor XI: stdout is primary; journald is added when available (daemon).
    let journald = tracing_journald::layer().ok();

    tracing_subscriber::registry()
        .with(filter)
        .with(tracing_subscriber::fmt::layer())
        .with(journald)
        .init();
}
