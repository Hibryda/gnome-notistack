//! Accessibility: AT-SPI2 emission for screen readers (Orca) (M5).
//!
//! Promoted to **V1** per the tribunal arbiter's dissent + the Hume audit, which
//! flagged framing the a11y loss as "acceptable" a smuggled normative claim.
//! Override-redirect popups are invisible to AT-SPI by default, so the daemon
//! must self-register and emit announcements for each shown notification.
