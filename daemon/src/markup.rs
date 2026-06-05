//! FDO body-markup → Pango-markup translation (M4).
//!
//! The FDO spec permits `<b> <i> <u> <a href> <img>` plus XML entities. Pango
//! natively supports `<b>/<i>/<u>/<a href>`, so those pass through; `<img>` is
//! unsupported and is replaced by its `alt` text. The result is validated with
//! `pango::parse_markup`; on failure the body is treated as plain text (escaped)
//! so a malformed body can never break rendering (plan OBJ-40, rule 02).

/// Escape text so it is safe as Pango markup (and XML).
pub fn escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '\'' => out.push_str("&#39;"),
            '"' => out.push_str("&quot;"),
            _ => out.push(c),
        }
    }
    out
}

/// Translate FDO body markup into valid Pango markup, or fall back to escaped
/// plain text if the (translated) body is not valid markup.
pub fn to_pango(body: &str) -> String {
    let translated = strip_img(body);
    match ::pango::parse_markup(&translated, '\u{0}') {
        Ok(_) => translated,
        Err(_) => escape(body),
    }
}

/// Replace `<img .../>` (unsupported by Pango) with its `alt` text, if any.
fn strip_img(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut rest = s;
    while let Some(start) = rest.find("<img") {
        out.push_str(&rest[..start]);
        match rest[start..].find('>') {
            Some(rel_end) => {
                let tag = &rest[start..start + rel_end + 1];
                if let Some(alt) = extract_attr(tag, "alt") {
                    out.push_str(&escape(&alt));
                }
                rest = &rest[start + rel_end + 1..];
            }
            None => {
                rest = "";
                break;
            }
        }
    }
    out.push_str(rest);
    out
}

/// Extract a quoted attribute value (`name="value"` or `name='value'`) from a tag.
fn extract_attr(tag: &str, attr: &str) -> Option<String> {
    let key = format!("{attr}=");
    let i = tag.find(&key)? + key.len();
    let quote = tag.as_bytes().get(i).copied()?;
    if quote != b'"' && quote != b'\'' {
        return None;
    }
    let start = i + 1;
    let end = tag[start..].find(quote as char)? + start;
    Some(tag[start..end].to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn escapes_xml_specials() {
        assert_eq!(escape("a<b>&\"'"), "a&lt;b&gt;&amp;&quot;&#39;");
    }

    #[test]
    fn passes_valid_markup() {
        assert_eq!(to_pango("<b>bold</b> <i>it</i>"), "<b>bold</b> <i>it</i>");
        assert_eq!(to_pango("a &amp; b"), "a &amp; b");
    }

    #[test]
    fn falls_back_on_invalid_markup() {
        // A bare ampersand is invalid markup → escaped plaintext fallback.
        assert_eq!(to_pango("Tom & Jerry"), "Tom &amp; Jerry");
        // An unclosed tag is invalid → fallback.
        assert_eq!(to_pango("<b>oops"), "&lt;b&gt;oops");
    }

    #[test]
    fn replaces_img_with_alt() {
        assert_eq!(
            to_pango("see <img src=\"x.png\" alt=\"a cat\"/> here"),
            "see a cat here"
        );
        assert_eq!(to_pango("<img src=\"x.png\"/>only"), "only");
    }
}
