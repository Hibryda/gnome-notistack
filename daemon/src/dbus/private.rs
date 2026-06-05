//! Private control interface for the `notistack-ctl` companion and the shell
//! extension's handshake telemetry (M1+).
//!
//! Security (rule 01): mutating control methods must re-query the caller PID via
//! `GetConnectionUnixProcessID` per call (plan OBJ / risk R10) — never trust a
//! cached PID.

use zbus::interface;

#[derive(Default)]
pub struct Control {}

#[interface(name = "store.hemoglobina.notistack.Control")]
impl Control {
    /// Liveness probe used by the extension's `enable()` fast-path (plan risk R4).
    fn is_ready(&self) -> bool {
        true
    }

    /// The extension reports handshake progress/errors here (typed vocabulary, M1).
    fn report_handshake_event(&self, kind: String, detail: String) {
        let _ = (kind, detail); // M1: structured tracing + state transitions.
    }
}
