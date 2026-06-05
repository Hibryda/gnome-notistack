//! Async X11 event loop (M2).
//!
//! Drives the popup windows: `AsyncFd(conn.stream())` + `poll_for_event` in a
//! `select!` loop; `XSelectInput` for `ButtonPress`/`Enter`/`Leave`; hit-tests
//! pointer events against the per-card click regions from `render::layout`, then
//! dispatches `ActionInvoked` / `CloseNotification` (plan OBJ-50).

/// Run the X11 event loop until cancelled. M2 implements the body.
pub async fn run() -> crate::error::Result<()> {
    // M2: AsyncFd over the XCB fd + poll_for_event; ButtonPress hit-testing.
    Ok(())
}
