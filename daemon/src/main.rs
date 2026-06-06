//! gnome-notistack — standalone FreeDesktop notification daemon for GNOME Shell 48 / X11.
//!
//! Shows multiple notifications at once as a vertical stack of override-redirect
//! ARGB popups (dunst-style, on GNOME). See `docs/IMPLEMENTATION-PLAN.md`.
//!
//! `// M<n>:` comments mark where each milestone's behavior lands.

mod a11y;
mod bus;
mod cli;
mod config;
mod dbus;
mod history;
mod lockscreen;
mod markup;
mod notification;
mod render;
mod shutdown;
mod sound;
mod suppression;

use anyhow::Context;
use tracing::{error, info};

fn main() -> anyhow::Result<()> {
    use clap::Parser;
    init_tracing();

    let cli = cli::Cli::parse();
    // Config comes from GSettings (live-reloaded by the render thread).
    let config = config::Config::load();

    // Standalone visual smoke test (no D-Bus takeover needed).
    if cli.demo_popup {
        return render::demo(&config);
    }

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

    // Watch DND (gsettings) + screen lock and suppress accordingly.
    tokio::spawn(suppression::run(conn.clone(), tx.clone()));

    shutdown::wait_for_shutdown().await;
    info!("gnome-notistack shutting down");
    let _ = tx.send(render::Command::Shutdown);
    drop(tx);
    let _ = render_thread.join();
    Ok(())
}

/// Invoke a GTK notification's default action on its app via
/// `org.freedesktop.Application.ActivateAction` (the `app.` prefix is stripped).
async fn activate_gtk_action(conn: &zbus::Connection, app_id: &str, action: &str) {
    use std::collections::HashMap;
    use zbus::zvariant::Value;

    let object_path = format!("/{}", app_id.replace('.', "/"));
    let action_name = action.strip_prefix("app.").unwrap_or(action);
    let body = (
        action_name,
        Vec::<Value>::new(),
        HashMap::<String, Value>::new(),
    );
    if let Err(e) = conn
        .call_method(
            Some(app_id),
            object_path.as_str(),
            Some("org.freedesktop.Application"),
            "ActivateAction",
            &body,
        )
        .await
    {
        error!(app_id, action, error = %e, "GTK ActivateAction failed");
    }
}

/// Open a body hyperlink via `xdg-open`, restricted to safe web/mail schemes so a
/// notification can't trigger `file://` or arbitrary scheme handlers (rule 01).
fn open_url(url: &str) {
    let ok =
        url.starts_with("http://") || url.starts_with("https://") || url.starts_with("mailto:");
    if !ok {
        error!(url, "refusing to open non-web URL from a notification");
        return;
    }
    let _ = std::process::Command::new("xdg-open")
        .arg(url)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn();
}

/// Drain the feedback channel and emit the matching FDO signals on the session bus.
fn spawn_signal_emitter(
    conn: zbus::Connection,
    mut fb_rx: tokio::sync::mpsc::UnboundedReceiver<render::Feedback>,
) {
    use zbus::object_server::SignalEmitter;

    let sound_backend = sound::detect();
    tokio::spawn(async move {
        let emitter = match SignalEmitter::new(&conn, dbus::FDO_PATH) {
            Ok(e) => e,
            Err(e) => {
                error!(error = %e, "failed to build FDO signal emitter");
                return;
            }
        };
        while let Some(fb) = fb_rx.recv().await {
            match fb {
                render::Feedback::Closed { id, reason } => {
                    if let Err(e) = dbus::fdo::emit_closed(&emitter, id, reason).await {
                        error!(error = %e, "failed to emit NotificationClosed");
                    }
                }
                render::Feedback::Action { id, key } => {
                    if let Err(e) = dbus::fdo::emit_action(&emitter, id, key).await {
                        error!(error = %e, "failed to emit ActionInvoked");
                    }
                }
                render::Feedback::PlaySound { file, name } => {
                    if let Some((prog, args)) =
                        sound::play_command(sound_backend, file.as_deref(), name.as_deref())
                    {
                        // Fire-and-forget; tokio reaps the dropped child.
                        let _ = tokio::process::Command::new(prog)
                            .args(args)
                            .stdin(std::process::Stdio::null())
                            .stdout(std::process::Stdio::null())
                            .stderr(std::process::Stdio::null())
                            .spawn();
                    }
                }
                render::Feedback::GtkActivate { app_id, action } => {
                    activate_gtk_action(&conn, &app_id, &action).await;
                }
                render::Feedback::OpenUrl(url) => open_url(&url),
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
