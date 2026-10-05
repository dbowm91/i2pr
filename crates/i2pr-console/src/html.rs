//! Escaped-text primitives for server-rendered markup.
//!
//! Router-derived values reach HTML only through [`Text`], whose
//! constructor escapes. There is no raw-string path into a response body,
//! so a hostile RouterInfo field cannot introduce markup, an attribute, or
//! a second document.

use std::fmt;

/// Escapes the five HTML-significant bytes in `raw`.
///
/// `&`, `<`, `>`, `"`, and `'` are all replaced. Ampersand is replaced
/// first so already-escaped output is never double-decoded by a browser.
pub fn escape_html(raw: &str) -> String {
    let mut escaped = String::with_capacity(raw.len() + raw.len() / 8);
    for character in raw.chars() {
        match character {
            '&' => escaped.push_str("&amp;"),
            '<' => escaped.push_str("&lt;"),
            '>' => escaped.push_str("&gt;"),
            '"' => escaped.push_str("&quot;"),
            '\'' => escaped.push_str("&#x27;"),
            other => escaped.push(other),
        }
    }
    escaped
}

/// A string that is already HTML-escaped and safe to interpolate.
///
/// Construction escapes exactly once. `Display` emits the stored escaped
/// form verbatim, so a `Text` value can be concatenated into a template
/// without a second, inconsistent escaping pass.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Text(String);

impl Text {
    /// Escapes `raw` into an interpolatable value.
    pub fn new(raw: &str) -> Self {
        Self(escape_html(raw))
    }

    /// Returns the escaped bytes.
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// Returns the byte length of the escaped form.
    pub fn len(&self) -> usize {
        self.0.len()
    }

    /// Returns whether the escaped form is empty.
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

impl fmt::Display for Text {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn escapes_all_significant_bytes() {
        assert_eq!(escape_html("<"), "&lt;");
        assert_eq!(escape_html(">"), "&gt;");
        assert_eq!(escape_html("&"), "&amp;");
        assert_eq!(escape_html("\""), "&quot;");
        assert_eq!(escape_html("'"), "&#x27;");
    }

    #[test]
    fn ampersand_is_escaped_first_so_output_is_not_decodable() {
        // Pre-escaped input must not survive as a live entity.
        assert_eq!(escape_html("&lt;"), "&amp;lt;");
        assert_eq!(escape_html("&amp;"), "&amp;amp;");
    }

    #[test]
    fn markup_injection_attempts_are_neutralized() {
        let hostile = "<script>alert('x')</script>";
        let escaped = escape_html(hostile);
        assert!(!escaped.contains('<'));
        assert!(!escaped.contains('>'));
        assert!(!escaped.contains('\''));
        assert_eq!(escaped, "&lt;script&gt;alert(&#x27;x&#x27;)&lt;/script&gt;");
    }

    #[test]
    fn text_display_emits_the_escaped_form_once() {
        let value = Text::new("<b>router</b>");
        assert_eq!(value.to_string(), "&lt;b&gt;router&lt;/b&gt;");
        // Displaying twice does not double-escape.
        assert_eq!(value.to_string(), value.as_str());
    }

    #[test]
    fn empty_and_plain_values_are_unchanged() {
        assert_eq!(escape_html(""), "");
        assert_eq!(escape_html("abc-123_./"), "abc-123_./");
        assert!(Text::new("").is_empty());
        assert_eq!(Text::new("abc").len(), 3);
    }

    #[test]
    fn multibyte_text_passes_through_unchanged() {
        assert_eq!(
            escape_html("\u{00e9}\u{4e2d}\u{6587}"),
            "\u{00e9}\u{4e2d}\u{6587}"
        );
        assert_eq!(escape_html("caf\u{e9}"), "caf\u{e9}");
    }
}
