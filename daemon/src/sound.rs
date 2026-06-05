//! Best-effort notification sound (M4).
//!
//! A startup probe picks an available backend; `sound-file` hints play via
//! paplay, `sound-name` via canberra-gtk-play. Playback is fire-and-forget and
//! never blocks the daemon; failures log at debug (plan OBJ-52, risk R13).

/// Sound backend discovered at startup, in preference order.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SoundBackend {
    Paplay,
    CanberraGtkPlay,
    Aplay,
    None,
}
