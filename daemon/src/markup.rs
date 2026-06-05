//! FDO body-markup → Pango-markup translation (M4).
//!
//! The FDO spec permits `<b> <i> <u> <a href> <img>`. Pango markup differs:
//! `<u>` → `<span underline='single'>`, `<a href>` → its text, `<img>` → `[alt]`;
//! unknown tags are stripped, then `pango::parse_markup` validates with a
//! plain-text fallback (plan OBJ-40). Unit-tested.

/// Translate FDO body markup into Pango markup. M4 implements the real
/// translation + validation; the scaffold passes text through unchanged.
pub fn to_pango(body: &str) -> String {
    // M4: tag translation + pango::parse_markup validation with plaintext fallback.
    body.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn passthrough_is_identity_for_now() {
        // M4 will replace this with real-translation assertions.
        assert_eq!(to_pango("hello"), "hello");
    }
}
