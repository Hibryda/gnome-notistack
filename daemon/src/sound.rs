//! Best-effort notification sound (M4).
//!
//! A startup probe picks an available backend. The render thread decides *when*
//! to play (on display, so DND/queued notifications stay silent) and sends a
//! `Feedback::PlaySound`; the async feedback task spawns the player via
//! `tokio::process` (so the child is reaped). Failures are best-effort and never
//! block (plan OBJ-52, risk R13).

use tracing::debug;

/// Sound backend discovered at startup, in preference order.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Backend {
    Paplay,
    CanberraGtkPlay,
    Aplay,
    None,
}

/// Probe `$PATH` for a usable player. canberra-gtk-play is preferred because it
/// can also play themed sound *names* (`sound-name` hint), not just files.
pub fn detect() -> Backend {
    let backend = if which("canberra-gtk-play") {
        Backend::CanberraGtkPlay
    } else if which("paplay") {
        Backend::Paplay
    } else if which("aplay") {
        Backend::Aplay
    } else {
        Backend::None
    };
    debug!(?backend, "sound backend detected");
    backend
}

/// Build the `(program, args)` to play a `sound-file` (preferred) or themed
/// `sound-name`, or `None` if nothing can be played.
pub fn play_command(
    backend: Backend,
    file: Option<&str>,
    name: Option<&str>,
) -> Option<(String, Vec<String>)> {
    if backend == Backend::None {
        return None;
    }
    if let Some(f) = file {
        return Some(match backend {
            Backend::Paplay => ("paplay".into(), vec![f.into()]),
            Backend::CanberraGtkPlay => ("canberra-gtk-play".into(), vec!["-f".into(), f.into()]),
            Backend::Aplay => ("aplay".into(), vec!["-q".into(), f.into()]),
            Backend::None => unreachable!(),
        });
    }
    // Only canberra resolves themed sound names.
    name.map(|n| ("canberra-gtk-play".into(), vec!["-i".into(), n.into()]))
}

fn which(bin: &str) -> bool {
    std::env::var_os("PATH")
        .map(|paths| std::env::split_paths(&paths).any(|p| p.join(bin).is_file()))
        .unwrap_or(false)
}
