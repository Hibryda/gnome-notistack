//! Queue-based bus-name acquisition + re-acquire state machine (M1).
//!
//! Audit-driven design (see `docs/gnome48-audit.md`): the daemon requests each
//! well-known name with `REPLACE_EXISTING` and **sits queued** — it never sets
//! `ALLOW_REPLACEMENT` (which would let a restarted shell re-steal the name) and
//! never sets `DO_NOT_QUEUE` (we rely on D-Bus promoting us when the companion
//! extension releases the shell's ownership via `ReleaseName`).
//!
//! This queue-based model subsumes the watch-before-kill handshake and is the
//! primary defense for the fast-shell-restart race (plan risk R4): a restarted
//! shell calling `own_name(REPLACE)` cannot evict a non-replaceable owner.

use std::time::Duration;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum State {
    Initializing,
    /// Queued behind the current owner, waiting for the extension to release it.
    Queued,
    /// We hold the name and serve notifications.
    OwnsName,
    /// Lost the name (e.g. full shell restart); re-queuing to re-acquire.
    Reacquiring,
}

/// How long to wait for the extension handshake before falling back to a direct
/// (degraded, documented) acquisition attempt. See plan risks R4 / R16.
pub const HANDSHAKE_TIMEOUT: Duration = Duration::from_secs(10);
