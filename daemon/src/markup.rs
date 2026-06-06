//! FDO body-markup → Pango-markup translation (M4).
//!
//! The FDO spec permits `<b> <i> <u> <a href> <img>` plus XML entities and `<br>`.
//! Pango's `parse_markup` supports `<b>/<i>/<u>` but NOT `<a>` (that's a GtkLabel
//! feature) or `<img>`: so `<a href>` is rewritten to an underlined colored
//! `<span>` (with the URL tracked separately via [`extract_links`] for clicks),
//! `<img>` is replaced by its `alt` (and rendered separately), and `<br>` becomes
//! a newline. The result is validated with `pango::parse_markup`; on failure the
//! body is treated as plain text (escaped) so a malformed body can never break
//! rendering (plan OBJ-40, rule 02).

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
/// plain text if the (translated) body is not valid markup. `<br>` (not a Pango
/// tag, but common) becomes a newline; `<img>` becomes its `alt`.
pub fn to_pango(body: &str) -> String {
    let brs = convert_br(body);
    let translated = strip_img(&convert_anchors(&brs));
    match ::pango::parse_markup(&translated, '\u{0}') {
        Ok(_) => translated,
        Err(_) => escape(&brs), // keep line breaks even in the plaintext fallback
    }
}

/// Rewrite `<a href="url">text</a>` to an underlined, colored `<span>` (Pango's
/// `parse_markup` doesn't understand `<a>`). The URL is recovered separately by
/// [`extract_links`] for click handling.
fn convert_anchors(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut rest = s;
    while let Some(start) = rest.find("<a ") {
        out.push_str(&rest[..start]);
        let after = &rest[start..];
        let Some(gt) = after.find('>') else {
            out.push_str(after);
            return out;
        };
        let inner_rest = &after[gt + 1..];
        match inner_rest.find("</a>") {
            Some(close) => {
                out.push_str("<span underline=\"single\" foreground=\"#3584e4\">");
                out.push_str(&inner_rest[..close]);
                out.push_str("</span>");
                rest = &inner_rest[close + 4..];
            }
            None => {
                out.push_str(inner_rest);
                return out;
            }
        }
    }
    out.push_str(rest);
    out
}

/// Replace `<br>` / `<br/>` / `<br ...>` (case-insensitive) with a newline.
fn convert_br(s: &str) -> String {
    let lower = s.to_ascii_lowercase();
    let mut out = String::with_capacity(s.len());
    let mut i = 0;
    while i < s.len() {
        if lower[i..].starts_with("<br") {
            let after = lower.as_bytes().get(i + 3).copied().unwrap_or(b'>');
            if (after == b'>' || after == b'/' || after == b' ') && s[i..].contains('>') {
                let rel = s[i..].find('>').unwrap();
                out.push('\n');
                i += rel + 1;
                continue;
            }
        }
        let ch = s[i..].chars().next().unwrap();
        out.push(ch);
        i += ch.len_utf8();
    }
    out
}

/// Unescape the XML entities `escape()` produces (for matching Pango's text).
fn unescape(s: &str) -> String {
    s.replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&#39;", "'")
        .replace("&amp;", "&")
}

/// Strip all `<...>` tags from a fragment (for a link's visible text).
fn strip_tags(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut rest = s;
    while let Some(start) = rest.find('<') {
        out.push_str(&rest[..start]);
        match rest[start..].find('>') {
            Some(rel) => rest = &rest[start + rel + 1..],
            None => {
                rest = "";
                break;
            }
        }
    }
    out.push_str(rest);
    out
}

/// Extract `<a href="url">text</a>` links as `(url, visible_text)`, with the
/// visible text matching what Pango renders (tags stripped, entities decoded) —
/// used to map clicks back to URLs. `<br>` in link text becomes a newline first.
pub fn extract_links(body: &str) -> Vec<(String, String)> {
    let body = convert_br(body);
    let mut out = Vec::new();
    let mut rest = body.as_str();
    while let Some(start) = rest.find("<a ") {
        let after = &rest[start..];
        let Some(gt) = after.find('>') else { break };
        let tag = &after[..gt + 1];
        let inner_rest = &after[gt + 1..];
        let (inner, advance) = match inner_rest.find("</a>") {
            Some(close) => (&inner_rest[..close], gt + 1 + close + 4),
            None => (inner_rest, after.len()),
        };
        if let Some(url) = extract_attr(tag, "href") {
            out.push((url, unescape(&strip_tags(inner))));
        }
        rest = &after[advance..];
    }
    out
}

/// Extract `<img src="...">` sources from the body (for inline image rendering).
pub fn extract_images(body: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut rest = body;
    while let Some(start) = rest.find("<img") {
        match rest[start..].find('>') {
            Some(rel) => {
                let tag = &rest[start..start + rel + 1];
                if let Some(src) = extract_attr(tag, "src") {
                    out.push(src);
                }
                rest = &rest[start + rel + 1..];
            }
            None => break,
        }
    }
    out
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
    fn converts_br_to_newline() {
        assert_eq!(to_pango("a<br>b"), "a\nb");
        assert_eq!(to_pango("a<br/>b<br />c"), "a\nb\nc");
        assert_eq!(to_pango("<b>x</b><br>y"), "<b>x</b>\ny");
    }

    #[test]
    fn converts_anchor_to_span() {
        assert_eq!(
            to_pango("Visit <a href=\"https://x\">site</a>"),
            "Visit <span underline=\"single\" foreground=\"#3584e4\">site</span>"
        );
    }

    #[test]
    fn extracts_links() {
        assert_eq!(
            extract_links("see <a href=\"https://x.test\">the site</a> now"),
            vec![("https://x.test".to_string(), "the site".to_string())]
        );
        assert!(extract_links("no links here").is_empty());
    }

    #[test]
    fn extracts_images() {
        assert_eq!(
            extract_images("a <img src=\"/tmp/x.png\" alt=\"x\"/> b"),
            vec!["/tmp/x.png".to_string()]
        );
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
