//! Bounded color parsing, derivation, and contrast helpers.
//!
//! Every value that reaches a CSS declaration passes through this module as an
//! opaque [`Rgb`]. The theme adapter never splices a source string into CSS: it
//! converts to six hex digits and emits only `#rrggbb`. An unparseable or
//! out-of-bounds value becomes a typed [`ColorError`], never raw text.

use std::fmt;

/// Maximum accepted length of one color literal.
///
/// `#rrggbb` is six digits plus the leading `#`; the eight-digit
/// `#rrggbbaa` form is accepted only so its alpha channel can be
/// discarded for opaque UI surfaces. Anything longer is rejected before
/// inspection so a hostile theme cannot widen parser work.
pub const MAX_COLOR_LITERAL_LEN: usize = 9;

/// Minimum contrast ratio required for body text on a surface.
///
/// This is the WCAG 2.1 AA threshold for normal-size text. The console
/// never renders a theme below it: see
/// [`Rgb::ensure_contrast`].
pub const MIN_TEXT_CONTRAST: f64 = 4.5;

/// Minimum contrast ratio required for large/bold text and UI borders.
pub const MIN_LARGE_TEXT_CONTRAST: f64 = 3.0;

/// An opaque 8-bit-per-channel RGB color.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Rgb {
    /// Red channel.
    pub r: u8,
    /// Green channel.
    pub g: u8,
    /// Blue channel.
    pub b: u8,
}

impl Rgb {
    /// Builds a color from explicit channels.
    pub const fn new(r: u8, g: u8, b: u8) -> Self {
        Self { r, g, b }
    }

    /// Parses `#rrggbb` or `#rrggbbaa`.
    ///
    /// Alpha is accepted and discarded: the console renders opaque
    /// surfaces, and silently compositing an attacker-chosen alpha would
    /// change contrast in ways the readability gate cannot reason about.
    /// The leading `#` is mandatory, hex digits must be lowercase or
    /// uppercase ASCII, and the literal must be exactly 6 or 8 digits.
    pub fn parse_hex(literal: &str) -> Result<Self, ColorError> {
        if literal.len() > MAX_COLOR_LITERAL_LEN {
            return Err(ColorError::TooLong);
        }
        let Some(digits) = literal.strip_prefix('#') else {
            return Err(ColorError::MissingHash);
        };
        if !digits.bytes().all(|byte| byte.is_ascii_hexdigit()) {
            return Err(ColorError::NotHex);
        }
        // Only 6- and 8-digit forms are recognized. A 3-digit shorthand is
        // rejected rather than expanded so the accepted grammar stays
        // closed and testable.
        if !matches!(digits.len(), 6 | 8) {
            return Err(ColorError::UnsupportedLength);
        }
        let mut channels = [0u8; 3];
        for (index, channel) in channels.iter_mut().enumerate() {
            let offset = index * 2;
            let pair = &digits[offset..offset + 2];
            let value = u8::from_str_radix(pair, 16).map_err(|_| ColorError::NotHex)?;
            *channel = value;
        }
        Ok(Self::new(channels[0], channels[1], channels[2]))
    }

    /// Renders the canonical six-digit lowercase CSS form.
    ///
    /// This is the only representation the theme adapter emits. It contains
    /// no delimiter, quote, or backslash, so it cannot terminate or open a
    /// CSS declaration.
    pub fn to_css_hex(self) -> String {
        format!("#{:02x}{:02x}{:02x}", self.r, self.g, self.b)
    }

    /// Perceived luminance using the WCAG 2.1 sRGB formula.
    pub fn relative_luminance(self) -> f64 {
        fn channel(value: u8) -> f64 {
            let raw = f64::from(value) / 255.0;
            if raw <= 0.040_45 {
                raw / 12.92
            } else {
                ((raw + 0.055) / 1.055).powf(2.4)
            }
        }
        0.2126 * channel(self.r) + 0.7152 * channel(self.g) + 0.0722 * channel(self.b)
    }

    /// Symmetric contrast ratio against another color, in `1.0..=21.0`.
    pub fn contrast_ratio(self, other: Rgb) -> f64 {
        let first = self.relative_luminance();
        let second = other.relative_luminance();
        let lighter = first.max(second);
        let darker = first.min(second);
        (lighter + 0.05) / (darker + 0.05)
    }

    /// Returns this color mixed toward `other` by `weight` in `0.0..=1.0`.
    ///
    /// Rounding is round-half-up on each channel, so the result is a pure
    /// function of the inputs and is stable across platforms.
    pub fn mix(self, other: Rgb, weight: f64) -> Self {
        let weight = weight.clamp(0.0, 1.0);
        let blend = |left: u8, right: u8| -> u8 {
            let left = f64::from(left);
            let right = f64::from(right);
            let mixed = left + (right - left) * weight;
            mixed.round().clamp(0.0, 255.0) as u8
        };
        Self::new(
            blend(self.r, other.r),
            blend(self.g, other.g),
            blend(self.b, other.b),
        )
    }

    /// Returns this color mixed toward white by `weight`.
    pub fn lighten(self, weight: f64) -> Self {
        self.mix(Self::new(0xff, 0xff, 0xff), weight)
    }

    /// Returns this color mixed toward black by `weight`.
    pub fn darken(self, weight: f64) -> Self {
        self.mix(Self::new(0x00, 0x00, 0x00), weight)
    }

    /// Returns `self` when it already meets `minimum` against `background`,
    /// otherwise the deterministic repair.
    ///
    /// Repair walks `self` away from `background` in fixed 5% steps toward
    /// whichever of black/white increases contrast the most, and gives up
    /// after [`Self::MAX_REPAIR_STEPS`] iterations by returning the best
    /// candidate. The function is total, deterministic, and never returns a
    /// color that is less contrasting than its input.
    pub fn ensure_contrast(self, background: Rgb, minimum: f64) -> Self {
        if self.contrast_ratio(background) >= minimum {
            return self;
        }
        let background_is_light = background.relative_luminance() > 0.5;
        let mut best = self;
        let mut best_ratio = self.contrast_ratio(background);
        for step in 1..=Self::MAX_REPAIR_STEPS {
            let weight = f64::from(step) * 0.05;
            let candidate = if background_is_light {
                self.darken(weight)
            } else {
                self.lighten(weight)
            };
            let ratio = candidate.contrast_ratio(background);
            if ratio > best_ratio {
                best = candidate;
                best_ratio = ratio;
            }
            if best_ratio >= minimum {
                break;
            }
        }
        // Both extremes are always available: one of black or white
        // reaches 21:1 against any background. The candidate must be
        // measured against `background` — the pairing that has to become
        // readable — not against the original foreground.
        if best_ratio >= minimum {
            return best;
        }
        let black = Self::new(0x00, 0x00, 0x00);
        let white = Self::new(0xff, 0xff, 0xff);
        if black.contrast_ratio(background) >= white.contrast_ratio(background) {
            black
        } else {
            white
        }
    }

    /// Number of fixed repair steps attempted before the extreme fallback.
    pub const MAX_REPAIR_STEPS: u8 = 20;
}

impl fmt::Display for Rgb {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.to_css_hex())
    }
}

/// Why one color literal was rejected.
///
/// The variants carry no source text: a caller cannot use an error to echo
/// untrusted theme content back to a browser or log.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ColorError {
    /// The literal exceeded [`MAX_COLOR_LITERAL_LEN`].
    TooLong,
    /// The leading `#` was absent.
    MissingHash,
    /// A non-hexadecimal byte was present.
    NotHex,
    /// The digit count was neither 6 nor 8.
    UnsupportedLength,
}

impl fmt::Display for ColorError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::TooLong => formatter.write_str("color literal exceeds the accepted length"),
            Self::MissingHash => formatter.write_str("color literal must start with '#'"),
            Self::NotHex => formatter.write_str("color literal must contain only hex digits"),
            Self::UnsupportedLength => {
                formatter.write_str("color literal must be #rrggbb or #rrggbbaa")
            }
        }
    }
}

impl std::error::Error for ColorError {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_six_and_eight_digit_forms_and_discards_alpha() {
        assert_eq!(Rgb::parse_hex("#000000"), Ok(Rgb::new(0, 0, 0)));
        assert_eq!(Rgb::parse_hex("#ffffff"), Ok(Rgb::new(255, 255, 255)));
        // Alpha is discarded so opaque surfaces keep computed contrast.
        assert_eq!(Rgb::parse_hex("#123456ff"), Ok(Rgb::new(0x12, 0x34, 0x56)));
        assert_eq!(Rgb::parse_hex("#12345600"), Ok(Rgb::new(0x12, 0x34, 0x56)));
        // Mixed case digits are accepted.
        assert_eq!(Rgb::parse_hex("#AbCdEf"), Ok(Rgb::new(0xab, 0xcd, 0xef)));
    }

    #[test]
    fn rejects_out_of_grammar_literals() {
        // No leading hash.
        assert_eq!(Rgb::parse_hex("ffffff"), Err(ColorError::MissingHash));
        // Three-digit shorthand is not silently expanded.
        assert_eq!(Rgb::parse_hex("#fff"), Err(ColorError::UnsupportedLength));
        // Seven and nine digits are outside the grammar.
        assert_eq!(
            Rgb::parse_hex("#fffffff"),
            Err(ColorError::UnsupportedLength)
        );
        assert_eq!(Rgb::parse_hex("#fffffffff"), Err(ColorError::TooLong));
        // Non-hex bytes.
        assert_eq!(Rgb::parse_hex("#gggggg"), Err(ColorError::NotHex));
        // Empty and whitespace.
        assert_eq!(Rgb::parse_hex(""), Err(ColorError::MissingHash));
        assert_eq!(Rgb::parse_hex("#12345 "), Err(ColorError::NotHex));
    }

    #[test]
    fn css_form_is_exactly_six_lowercase_digits() {
        let css = Rgb::new(0x0a, 0xb1, 0xff).to_css_hex();
        assert_eq!(css, "#0ab1ff");
        assert_eq!(css.len(), 7);
        assert!(
            css.bytes()
                .all(|byte| byte == b'#' || byte.is_ascii_hexdigit())
        );
    }

    #[test]
    fn contrast_ratio_matches_wcag_reference_points() {
        let black = Rgb::new(0, 0, 0);
        let white = Rgb::new(255, 255, 255);
        assert!((black.contrast_ratio(white) - 21.0).abs() < 1e-9);
        assert!((white.contrast_ratio(black) - 21.0).abs() < 1e-9);
        assert!(black.contrast_ratio(black) < 1.001);
    }

    #[test]
    fn contrast_is_symmetric() {
        let first = Rgb::new(0x10, 0x20, 0x30);
        let second = Rgb::new(0x40, 0x50, 0x60);
        assert_eq!(first.contrast_ratio(second), second.contrast_ratio(first));
    }

    #[test]
    fn mix_is_endpoints_and_pure() {
        let black = Rgb::new(0, 0, 0);
        let white = Rgb::new(255, 255, 255);
        assert_eq!(black.mix(white, 0.0), black);
        assert_eq!(black.mix(white, 1.0), white);
        assert_eq!(black.mix(white, 0.5), Rgb::new(128, 128, 128));
        // Repeated calls are identical.
        assert_eq!(white.mix(black, 0.25), white.mix(black, 0.25));
    }

    #[test]
    fn ensure_contrast_returns_input_when_already_readable() {
        let black = Rgb::new(0, 0, 0);
        let white = Rgb::new(255, 255, 255);
        assert_eq!(black.ensure_contrast(white, MIN_TEXT_CONTRAST), black);
    }

    #[test]
    fn ensure_contrast_repairs_low_contrast_pairs() {
        // Near-identical colors: the repair must reach the AA threshold.
        let background = Rgb::new(0x20, 0x20, 0x20);
        let foreground = Rgb::new(0x30, 0x30, 0x30);
        let repaired = foreground.ensure_contrast(background, MIN_TEXT_CONTRAST);
        assert_ne!(repaired, foreground);
        assert!(
            repaired.contrast_ratio(background) >= MIN_TEXT_CONTRAST,
            "repaired {} on {} ratio {}",
            repaired,
            background,
            repaired.contrast_ratio(background)
        );
    }

    #[test]
    fn ensure_contrast_is_total_and_deterministic() {
        for seed in 0u16..64 {
            let background = Rgb::new(
                (seed * 4) as u8,
                (seed * 7 % 256) as u8,
                (seed * 11 % 256) as u8,
            );
            // Every channel stays inside 0..=255 for the whole seed range,
            // so the sweep exercises real colors rather than truncated ones.
            let foreground = Rgb::new(
                (255u16 - seed * 3).min(255) as u8,
                (seed * 13 % 256) as u8,
                (255u16 - seed * 4).min(255) as u8,
            );
            for minimum in [MIN_TEXT_CONTRAST, MIN_LARGE_TEXT_CONTRAST] {
                let first = foreground.ensure_contrast(background, minimum);
                let second = foreground.ensure_contrast(background, minimum);
                assert_eq!(first, second, "repair must be deterministic");
                assert!(
                    first.contrast_ratio(background) >= foreground.contrast_ratio(background),
                    "repair must not reduce contrast"
                );
                // A repaired pair is always fully readable, because the
                // fallback chooses whichever extreme reaches 21:1.
                assert!(first.contrast_ratio(background) >= minimum);
            }
        }
    }

    #[test]
    fn ensure_contrast_never_worsens_an_already_passing_pair() {
        let background = Rgb::new(0xff, 0xff, 0xff);
        let foreground = Rgb::new(0x00, 0x00, 0x00);
        assert_eq!(
            foreground.ensure_contrast(background, MIN_TEXT_CONTRAST),
            foreground
        );
    }
}
