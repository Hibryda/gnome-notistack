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
mod history;
mod lockscreen;
mod markup;
mod name_watcher;
mod notification;
mod render;
mod shutdown;
mod sound;

use anyhow::Context;
use tracing::{error, info};

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
    // Forward channel: D-Bus handlers -> render thread (notifications to show).
    let (tx, rx) = tokio::sync::mpsc::unbounded_channel::<render::Command>();
    // Feedback channel: render thread -> async signal emitter (closes/actions).
    let (fb_tx, fb_rx) = tokio::sync::mpsc::unbounded_channel::<render::Feedback>();

    let render_config = config.clone();
    let render_thread = std::thread::Builder::new()
        .name("notistack-render".into())
        .spawn(move || {
            if let Err(e) = render::manager::run(rx, fb_tx, render_config) {
                error!(error = %e, "render thread exited with error");
            }
        })
        .context("spawning render thread")?;

    // Serve the D-Bus interfaces and queue for the notification name(s). The
    // companion extension frees the names; D-Bus then promotes us to owner.
    let conn = bus::serve(&config, tx.clone()).await?;
    info!("serving D-Bus; queued for the notification name(s) awaiting release");

    spawn_signal_emitter(conn.clone(), fb_rx);

    shutdown::wait_for_shutdown().await;
    info!("gnome-notistack shutting down");
    let _ = tx.send(render::Command::Shutdown);
    drop(tx);
    let _ = render_thread.join();
    Ok(())
}

/// Drain the feedback channel and emit the matching FDO signals on the session bus.
fn spawn_signal_emitter(
    conn: zbus::Connection,
    mut fb_rx: tokio::sync::mpsc::UnboundedReceiver<render::Feedback>,
) {
    use zbus::object_server::SignalEmitter;

    tokio::spawn(async move {
        let emitter = match SignalEmitter::new(&conn, dbus::FDO_PATH) {
            Ok(e) => e,
            Err(e) => {
                error!(error = %e, "failed to build FDO signal emitter");
                return;
            }
        };
        while let Some(fb) = fb_rx.recv().await {
            let result = match fb {
                render::Feedback::Closed { id, reason } => {
                    dbus::fdo::emit_closed(&emitter, id, reason).await
                }
                render::Feedback::Action { id, key } => {
                    dbus::fdo::emit_action(&emitter, id, key).await
                }
            };
            if let Err(e) = result {
                error!(error = %e, "failed to emit FDO signal");
            }
        }
    });
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
