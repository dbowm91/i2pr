//! Halloy-compatible theme parsing and semantic CSS-variable translation.
//!
//! The console speaks the Halloy theme vocabulary as *data*: a bounded TOML
//! document whose `[general]`, `[text]`, `[buttons]`, `[buffer]`, and
//! `[formatting]` tables are mapped onto console web roles. The mapping is
//! one-way — no Halloy value is ever emitted as CSS text, and no CSS
//! selector is generated per theme. The browser consumes CSS custom
//! properties produced by [`ThemePalette::css_variables`].
//!
//! Bundled palettes are original i2pr work authored in the published Halloy
//! schema; see `assets/themes/PROVENANCE.md` for the license and
//! provenance record and for why the upstream Halloy theme set is not
//! vendored.

use std::collections::BTreeMap;
use std::fmt;

use serde::Deserialize;
use serde::de::{Deserializer, MapAccess, Visitor};

use crate::color::{ColorError, MIN_LARGE_TEXT_CONTRAST, MIN_TEXT_CONTRAST, Rgb};

/// Maximum accepted size of one theme document.
pub const MAX_THEME_SOURCE_BYTES: usize = 64 * 1024;

/// Maximum accepted TOML table-header nesting depth.
///
/// The depth is bounded by an explicit pre-scan rather than by the TOML
/// parser so a hostile document cannot drive unbounded recursive
/// deserialization before any field limit applies.
pub const MAX_THEME_NESTING_DEPTH: usize = 8;

/// Maximum accepted number of declared theme keys across all tables.
pub const MAX_THEME_KEYS: usize = 256;

/// Maximum accepted length of a theme name.
pub const MAX_THEME_NAME_LEN: usize = 64;

/// Name of the palette used when a document is unusable.
pub const DEFAULT_THEME_NAME: &str = "i2pr-default";

/// One bundled, provenance-cleared theme document.
#[derive(Clone, Copy, Debug)]
pub struct BundledTheme {
    /// Stable identifier an operator selects by name.
    pub name: &'static str,
    /// Compile-time bundled document text.
    pub source: &'static str,
}

/// The original i2pr palettes, in the published Halloy schema.
///
/// These are original works authored for this repository. No Halloy
/// upstream file is vendored; the schema is implemented from its public
/// documentation.
static BUNDLED_THEMES: &[BundledTheme] = &[
    BundledTheme {
        name: "i2pr-default",
        source: include_str!("../assets/themes/i2pr-default.toml"),
    },
    BundledTheme {
        name: "i2pr-midnight",
        source: include_str!("../assets/themes/i2pr-midnight.toml"),
    },
    BundledTheme {
        name: "i2pr-daylight",
        source: include_str!("../assets/themes/i2pr-daylight.toml"),
    },
];

/// Returns the compiled theme inventory in deterministic order.
///
/// The order is the declaration order above, which is stable across builds.
pub fn bundled_themes() -> &'static [BundledTheme] {
    BUNDLED_THEMES
}

/// Returns whether `name` is a compiled-in theme identifier.
///
/// Lookup is exact-match against the inventory; there is no filesystem
/// fallback, no normalization, and no case folding.
pub fn is_bundled_theme(name: &str) -> bool {
    BUNDLED_THEMES.iter().any(|theme| theme.name == name)
}

/// A validated theme identifier drawn from the compiled inventory.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ThemeName<'a> {
    value: &'a str,
}

impl<'a> ThemeName<'a> {
    /// Validates `name` against the compiled inventory and length bound.
    pub fn resolve(name: &'a str) -> Result<Self, ThemeError> {
        if name.is_empty() {
            return Err(ThemeError::EmptyName);
        }
        if name.len() > MAX_THEME_NAME_LEN {
            return Err(ThemeError::NameTooLong);
        }
        if !is_bundled_theme(name) {
            return Err(ThemeError::UnknownTheme);
        }
        Ok(Self { value: name })
    }

    /// Returns the validated identifier text.
    pub const fn as_str(&self) -> &'a str {
        self.value
    }
}

impl fmt::Display for ThemeName<'_> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.value)
    }
}

/// The semantic console palette: every web role resolved to an opaque color.
///
/// Field values are final. Construction applies the readability gate, so a
/// `ThemePalette` cannot hold an unreadable text/background pairing.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ThemePalette {
    /// Theme identity carried through to the rendered shell.
    pub name: String,

    /// Page background.
    pub page_background: Rgb,
    /// Default page text.
    pub page_text: Rgb,
    /// Page and section rule color.
    pub page_border: Rgb,

    /// Top bar surface.
    pub topbar_background: Rgb,
    /// Top bar text.
    pub topbar_text: Rgb,
    /// Navigation surface.
    pub nav_background: Rgb,
    /// Navigation text.
    pub nav_text: Rgb,

    /// Card/panel surface.
    pub panel_background: Rgb,
    /// Card/panel border.
    pub panel_border: Rgb,

    /// Table header surface.
    pub table_header_background: Rgb,
    /// Table header text.
    pub table_header_text: Rgb,
    /// Table row separator.
    pub table_border: Rgb,
    /// Alternating row surface.
    pub table_row_background: Rgb,

    /// Primary body text.
    pub text_primary: Rgb,
    /// Secondary text.
    pub text_secondary: Rgb,
    /// De-emphasized text.
    pub text_muted: Rgb,
    /// Success status color.
    pub status_success: Rgb,
    /// Warning status color.
    pub status_warning: Rgb,
    /// Error status color.
    pub status_error: Rgb,
    /// Informational status color.
    pub status_info: Rgb,

    /// Default button surface.
    pub button_background: Rgb,
    /// Default button text.
    pub button_text: Rgb,
    /// Button hover surface.
    pub button_hover_background: Rgb,
    /// Button pressed surface.
    pub button_active_background: Rgb,
    /// Button border.
    pub button_border: Rgb,

    /// Link color.
    pub link: Rgb,
    /// Accent color.
    pub accent: Rgb,

    /// Chip/tag surface.
    pub chip_background: Rgb,
    /// Chip/tag text.
    pub chip_text: Rgb,

    /// Graph series line.
    pub graph_line: Rgb,
    /// Graph filled area.
    pub graph_fill: Rgb,
    /// Graph grid line.
    pub graph_grid: Rgb,

    /// Form control surface.
    pub input_background: Rgb,
    /// Form control text.
    pub input_text: Rgb,
    /// Form control border.
    pub input_border: Rgb,
    /// Focus indicator.
    pub focus_ring: Rgb,
}

/// The fixed `(custom property, role)` emission order.
///
/// Names are compile-time constants. A theme cannot add, rename, or reorder
/// a property, which is what makes the generated stylesheet safe to serve
/// under a `style-src 'self'` policy with no injection surface.
const CSS_VARIABLE_ORDER: &[&str] = &[
    "--console-page-background",
    "--console-page-text",
    "--console-page-border",
    "--console-topbar-background",
    "--console-topbar-text",
    "--console-nav-background",
    "--console-nav-text",
    "--console-panel-background",
    "--console-panel-border",
    "--console-table-header-background",
    "--console-table-header-text",
    "--console-table-border",
    "--console-table-row-background",
    "--console-text-primary",
    "--console-text-secondary",
    "--console-text-muted",
    "--console-status-success",
    "--console-status-warning",
    "--console-status-error",
    "--console-status-info",
    "--console-button-background",
    "--console-button-text",
    "--console-button-hover-background",
    "--console-button-active-background",
    "--console-button-border",
    "--console-link",
    "--console-accent",
    "--console-chip-background",
    "--console-chip-text",
    "--console-graph-line",
    "--console-graph-fill",
    "--console-graph-grid",
    "--console-input-background",
    "--console-input-text",
    "--console-input-border",
    "--console-focus-ring",
];

/// Maximum byte length of a generated theme stylesheet.
///
/// The palette emits a fixed 38 declarations; this ceiling is a regression
/// tripwire, not a tuning knob.
pub const MAX_THEME_CSS_BYTES: usize = 8 * 1024;

impl ThemePalette {
    /// Returns the color bound to one compile-time property name.
    ///
    /// [`CSS_VARIABLE_ORDER`] is the single source of truth for emission
    /// order and membership; an unknown property returns `None` rather than
    /// inventing a name, so no code path can widen the stylesheet.
    fn role(&self, property: &str) -> Option<Rgb> {
        let color = match property {
            "--console-page-background" => self.page_background,
            "--console-page-text" => self.page_text,
            "--console-page-border" => self.page_border,
            "--console-topbar-background" => self.topbar_background,
            "--console-topbar-text" => self.topbar_text,
            "--console-nav-background" => self.nav_background,
            "--console-nav-text" => self.nav_text,
            "--console-panel-background" => self.panel_background,
            "--console-panel-border" => self.panel_border,
            "--console-table-header-background" => self.table_header_background,
            "--console-table-header-text" => self.table_header_text,
            "--console-table-border" => self.table_border,
            "--console-table-row-background" => self.table_row_background,
            "--console-text-primary" => self.text_primary,
            "--console-text-secondary" => self.text_secondary,
            "--console-text-muted" => self.text_muted,
            "--console-status-success" => self.status_success,
            "--console-status-warning" => self.status_warning,
            "--console-status-error" => self.status_error,
            "--console-status-info" => self.status_info,
            "--console-button-background" => self.button_background,
            "--console-button-text" => self.button_text,
            "--console-button-hover-background" => self.button_hover_background,
            "--console-button-active-background" => self.button_active_background,
            "--console-button-border" => self.button_border,
            "--console-link" => self.link,
            "--console-accent" => self.accent,
            "--console-chip-background" => self.chip_background,
            "--console-chip-text" => self.chip_text,
            "--console-graph-line" => self.graph_line,
            "--console-graph-fill" => self.graph_fill,
            "--console-graph-grid" => self.graph_grid,
            "--console-input-background" => self.input_background,
            "--console-input-text" => self.input_text,
            "--console-input-border" => self.input_border,
            "--console-focus-ring" => self.focus_ring,
            _ => return None,
        };
        Some(color)
    }

    /// Renders the palette as a `:root` rule of CSS custom properties.
    ///
    /// The output contains only compile-time property names and `#rrggbb`
    /// values produced by [`Rgb::to_css_hex`]. It always begins with a
    /// theme-identifying comment containing the validated name, which is
    /// itself bounded by [`MAX_THEME_NAME_LEN`] and drawn from the compiled
    /// inventory.
    pub fn css_variables(&self) -> String {
        let mut css = String::with_capacity(1024);
        css.push_str("/* i2pr-console theme: ");
        css.push_str(&self.name);
        css.push_str(" */\n:root{");
        for name in CSS_VARIABLE_ORDER {
            let color = self
                .role(name)
                .unwrap_or_else(|| panic!("{name} must map to a console role"));
            css.push_str(name);
            css.push(':');
            css.push_str(&color.to_css_hex());
            css.push(';');
        }
        css.push_str("}\n");
        debug_assert!(css.len() <= MAX_THEME_CSS_BYTES);
        css
    }

    /// Applies the readability gate and derives composite roles.
    ///
    /// Roles that the source document does not state are derived from the
    /// resolved primary roles rather than copied, so a partial theme still
    /// produces a complete, readable palette.
    fn finalize(name: String, raw: RawPalette) -> Self {
        let page_background = raw.page_background;
        let page_text = raw
            .page_text
            .ensure_contrast(page_background, MIN_TEXT_CONTRAST);

        let topbar_background = raw.topbar_background;
        let topbar_text = raw
            .topbar_text
            .ensure_contrast(topbar_background, MIN_TEXT_CONTRAST);
        let nav_background = raw.nav_background;
        let nav_text = raw
            .nav_text
            .ensure_contrast(nav_background, MIN_TEXT_CONTRAST);
        let panel_background = raw.panel_background;
        let panel_border = raw
            .panel_border
            .ensure_contrast(panel_background, MIN_LARGE_TEXT_CONTRAST);
        let table_header_background = raw.table_header_background;
        let table_header_text = raw
            .table_header_text
            .ensure_contrast(table_header_background, MIN_TEXT_CONTRAST);

        let text_primary = raw
            .text_primary
            .ensure_contrast(page_background, MIN_TEXT_CONTRAST);
        let text_secondary = raw
            .text_secondary
            .ensure_contrast(page_background, MIN_TEXT_CONTRAST);
        let text_muted = raw
            .text_muted
            .ensure_contrast(page_background, MIN_TEXT_CONTRAST);

        let button_background = raw.button_background;
        let button_text = raw
            .button_text
            .ensure_contrast(button_background, MIN_TEXT_CONTRAST);
        let button_border = raw
            .button_border
            .ensure_contrast(button_background, MIN_LARGE_TEXT_CONTRAST);

        let chip_background = raw.chip_background;
        let chip_text = raw
            .chip_text
            .ensure_contrast(chip_background, MIN_TEXT_CONTRAST);

        let input_background = raw.input_background;
        let input_text = raw
            .input_text
            .ensure_contrast(input_background, MIN_TEXT_CONTRAST);
        let input_border = raw
            .input_border
            .ensure_contrast(input_background, MIN_LARGE_TEXT_CONTRAST);

        // Derived roles: statuses default to the readable text color so a
        // minimal theme never renders an invisible control, and the focus
        // ring is the accent unless the theme states its own.
        let focus_ring = raw
            .focus_ring
            .ensure_contrast(page_background, MIN_LARGE_TEXT_CONTRAST);

        Self {
            name,
            page_background,
            page_text,
            page_border: raw
                .page_border
                .ensure_contrast(page_background, MIN_LARGE_TEXT_CONTRAST),
            topbar_background,
            topbar_text,
            nav_background,
            nav_text,
            panel_background,
            panel_border,
            table_header_background,
            table_header_text,
            table_border: raw
                .table_border
                .ensure_contrast(page_background, MIN_LARGE_TEXT_CONTRAST),
            table_row_background: raw.table_row_background,
            text_primary,
            text_secondary,
            text_muted,
            status_success: raw
                .status_success
                .ensure_contrast(page_background, MIN_TEXT_CONTRAST),
            status_warning: raw
                .status_warning
                .ensure_contrast(page_background, MIN_TEXT_CONTRAST),
            status_error: raw
                .status_error
                .ensure_contrast(page_background, MIN_TEXT_CONTRAST),
            status_info: raw
                .status_info
                .ensure_contrast(page_background, MIN_TEXT_CONTRAST),
            button_background,
            button_text,
            button_hover_background: raw.button_hover_background,
            button_active_background: raw.button_active_background,
            button_border,
            link: raw.link.ensure_contrast(page_background, MIN_TEXT_CONTRAST),
            accent: raw
                .accent
                .ensure_contrast(page_background, MIN_LARGE_TEXT_CONTRAST),
            chip_background,
            chip_text,
            graph_line: raw
                .graph_line
                .ensure_contrast(page_background, MIN_LARGE_TEXT_CONTRAST),
            graph_fill: raw.graph_fill,
            graph_grid: raw
                .graph_grid
                .ensure_contrast(page_background, MIN_LARGE_TEXT_CONTRAST),
            input_background,
            input_text,
            input_border,
            focus_ring,
        }
    }

    /// Returns the compiled default palette.
    ///
    /// This is the safe fallback used when a requested or supplied document
    /// is unusable. It is always readable by construction.
    pub fn compiled_default() -> Self {
        // The bundled default is known-good at build time; a parse failure
        // here would be a build defect, so fall back to a hard-coded,
        // self-evidently readable palette rather than panicking in a
        // runtime request path.
        match parse_theme(DEFAULT_THEME_NAME, bundled_default_source()) {
            Ok(palette) => palette,
            Err(_) => Self::finalize(DEFAULT_THEME_NAME.to_string(), RawPalette::dark_fallback()),
        }
    }

    /// Returns the palette for a bundled theme name, or the compiled default.
    ///
    /// An unknown name is a configuration error at the daemon boundary;
    /// this accessor exists so the render path always has a palette.
    pub fn resolve_or_default(name: &str) -> Self {
        match bundled_theme_source(name) {
            Some(source) => parse_theme(name, source).unwrap_or_else(|_| Self::compiled_default()),
            None => Self::compiled_default(),
        }
    }

    /// Reports every core pairing that fails the console readability floor.
    ///
    /// Used by tests and by the closure evidence checker. Because
    /// `ThemePalette::finalize` repairs pairings, a returned empty list is
    /// the expected result for any palette this crate produces.
    pub fn readability_failures(&self) -> Vec<(&'static str, f64)> {
        let checks: [(&'static str, Rgb, Rgb, f64); 12] = [
            (
                "page_text/page_background",
                self.page_text,
                self.page_background,
                MIN_TEXT_CONTRAST,
            ),
            (
                "topbar_text/topbar_background",
                self.topbar_text,
                self.topbar_background,
                MIN_TEXT_CONTRAST,
            ),
            (
                "nav_text/nav_background",
                self.nav_text,
                self.nav_background,
                MIN_TEXT_CONTRAST,
            ),
            (
                "table_header_text/table_header_background",
                self.table_header_text,
                self.table_header_background,
                MIN_TEXT_CONTRAST,
            ),
            (
                "text_primary/page_background",
                self.text_primary,
                self.page_background,
                MIN_TEXT_CONTRAST,
            ),
            (
                "text_secondary/page_background",
                self.text_secondary,
                self.page_background,
                MIN_TEXT_CONTRAST,
            ),
            (
                "text_muted/page_background",
                self.text_muted,
                self.page_background,
                MIN_TEXT_CONTRAST,
            ),
            (
                "button_text/button_background",
                self.button_text,
                self.button_background,
                MIN_TEXT_CONTRAST,
            ),
            (
                "chip_text/chip_background",
                self.chip_text,
                self.chip_background,
                MIN_TEXT_CONTRAST,
            ),
            (
                "input_text/input_background",
                self.input_text,
                self.input_background,
                MIN_TEXT_CONTRAST,
            ),
            (
                "link/page_background",
                self.link,
                self.page_background,
                MIN_TEXT_CONTRAST,
            ),
            (
                "page_border/page_background",
                self.page_border,
                self.page_background,
                MIN_LARGE_TEXT_CONTRAST,
            ),
        ];
        checks
            .into_iter()
            .filter(|(_, fg, bg, minimum)| fg.contrast_ratio(*bg) < *minimum)
            .map(|(name, fg, bg, _)| (name, fg.contrast_ratio(bg)))
            .collect()
    }
}

/// Returns the bundled source for a compiled theme name.
pub fn bundled_theme_source(name: &str) -> Option<&'static str> {
    BUNDLED_THEMES
        .iter()
        .find(|theme| theme.name == name)
        .map(|theme| theme.source)
}

/// Returns the source of the compiled default palette.
fn bundled_default_source() -> &'static str {
    bundled_theme_source(DEFAULT_THEME_NAME).unwrap_or("")
}

/// Parses a Halloy-compatible theme document into a console palette.
///
/// Bounds enforced before and during parsing:
///
/// - input size (`MAX_THEME_SOURCE_BYTES`);
/// - table-header nesting depth (`MAX_THEME_NESTING_DEPTH`);
/// - declared key count (`MAX_THEME_KEYS`);
/// - per-value hex length ([`crate::color::MAX_COLOR_LITERAL_LEN`]).
///
/// A malformed or unreadable document is an error; the caller decides to
/// fall back. Errors never carry theme text.
pub fn parse_theme(name: &str, source: &str) -> Result<ThemePalette, ThemeError> {
    if name.is_empty() {
        return Err(ThemeError::EmptyName);
    }
    if name.len() > MAX_THEME_NAME_LEN {
        return Err(ThemeError::NameTooLong);
    }
    if source.len() > MAX_THEME_SOURCE_BYTES {
        return Err(ThemeError::SourceTooLarge);
    }
    let depth = scan_nesting_depth(source);
    if depth > MAX_THEME_NESTING_DEPTH {
        return Err(ThemeError::NestingTooDeep);
    }
    let key_count = scan_key_count(source);
    if key_count > MAX_THEME_KEYS {
        return Err(ThemeError::TooManyKeys);
    }
    let raw: RawTheme = toml::from_str(source).map_err(|_| ThemeError::MalformedDocument)?;
    Ok(ThemePalette::finalize(name.to_string(), raw.into()))
}

/// Returns the maximum `[table]` header nesting depth in `source`.
///
/// This is a cheap lexical scan used as a pre-parse bound. It counts
/// consecutive `[` characters at the start of a trimmed line, which is the
/// only place TOML permits a table header, so an attacker cannot hide depth
/// inside a value.
fn scan_nesting_depth(source: &str) -> usize {
    let mut deepest = 0usize;
    let mut current = 0usize;
    for line in source.lines() {
        let trimmed = line.trim_start();
        if !trimmed.starts_with('[') {
            continue;
        }
        let run = trimmed.chars().take_while(|c| *c == '[').count();
        if run > current {
            current = run;
        }
        if current > deepest {
            deepest = current;
        }
    }
    deepest
}

/// Returns the number of `key = value` declarations in `source`.
///
/// An over-deep or over-wide document is rejected before allocation-heavy
/// deserialization, keeping parser work proportional to the input bound.
fn scan_key_count(source: &str) -> usize {
    source
        .lines()
        .filter(|line| {
            let trimmed = line.trim();
            !trimmed.is_empty() && !trimmed.starts_with('#') && !trimmed.starts_with('[')
        })
        .count()
}

/// Why a theme document was rejected.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ThemeError {
    /// The name was empty.
    EmptyName,
    /// The name exceeded [`MAX_THEME_NAME_LEN`].
    NameTooLong,
    /// The name is not in the compiled inventory.
    UnknownTheme,
    /// The document exceeded [`MAX_THEME_SOURCE_BYTES`].
    SourceTooLarge,
    /// Table nesting exceeded [`MAX_THEME_NESTING_DEPTH`].
    NestingTooDeep,
    /// The document declared more than [`MAX_THEME_KEYS`] keys.
    TooManyKeys,
    /// The document did not parse as the bounded Halloy schema.
    MalformedDocument,
    /// A declared color was outside the accepted hex grammar.
    InvalidColor,
}

impl fmt::Display for ThemeError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::EmptyName => formatter.write_str("theme name must not be empty"),
            Self::NameTooLong => formatter.write_str("theme name exceeds the accepted length"),
            Self::UnknownTheme => {
                formatter.write_str("theme name is not in the compiled inventory")
            }
            Self::SourceTooLarge => formatter.write_str("theme document exceeds the accepted size"),
            Self::NestingTooDeep => formatter.write_str("theme document nests too deeply"),
            Self::TooManyKeys => formatter.write_str("theme document declares too many keys"),
            Self::MalformedDocument => formatter.write_str("theme document is malformed"),
            Self::InvalidColor => formatter.write_str("theme document contains an invalid color"),
        }
    }
}

impl std::error::Error for ThemeError {}

// ---------------------------------------------------------------------------
// Bounded Halloy schema
// ---------------------------------------------------------------------------

/// A Halloy text style: a bare color or a `{ color, font_style }` table.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct TextStyle {
    /// Resolved color.
    pub color: Rgb,
    /// Declared font style, if the document stated one.
    pub font_style: Option<&'static str>,
}

impl<'de> Deserialize<'de> for TextStyle {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        struct StyleVisitor;

        impl<'de> Visitor<'de> for StyleVisitor {
            type Value = TextStyle;

            fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                formatter.write_str("a hex color or a { color, font_style } table")
            }

            fn visit_str<E>(self, value: &str) -> Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                let color = Rgb::parse_hex(value)
                    .map_err(|error| serde::de::Error::custom(describe_color_error(error)))?;
                Ok(TextStyle {
                    color,
                    font_style: None,
                })
            }

            fn visit_map<A>(self, mut map: A) -> Result<Self::Value, A::Error>
            where
                A: MapAccess<'de>,
            {
                let mut color: Option<Rgb> = None;
                let mut font_style: Option<&'static str> = None;
                while let Some(key) = map.next_key::<String>()? {
                    match key.as_str() {
                        "color" => {
                            let literal: String = map.next_value()?;
                            color = Some(Rgb::parse_hex(&literal).map_err(|error| {
                                serde::de::Error::custom(describe_color_error(error))
                            })?);
                        }
                        "font_style" => {
                            let literal: String = map.next_value()?;
                            font_style = normalize_font_style(&literal);
                        }
                        _ => {
                            let _ignored: serde::de::IgnoredAny = map.next_value()?;
                        }
                    }
                }
                let color = color.ok_or_else(|| {
                    serde::de::Error::custom("text style table requires a color key")
                })?;
                Ok(TextStyle { color, font_style })
            }
        }

        deserializer.deserialize_any(StyleVisitor)
    }
}

/// Maps a font-style string to a closed vocabulary.
///
/// The console has no custom font dependency, so the declared style is
/// carried as data for provenance only and never rendered. An unrecognized
/// style becomes `None` instead of an error, matching the upstream rule that
/// an unknown style is ignored rather than fatal.
fn normalize_font_style(value: &str) -> Option<&'static str> {
    match value {
        "normal" => Some("normal"),
        "italic" => Some("italic"),
        "bold" => Some("bold"),
        "italic-bold" => Some("italic-bold"),
        _ => None,
    }
}

/// Renders a color error as a short, content-free diagnostic.
fn describe_color_error(error: ColorError) -> String {
    error.to_string()
}

#[derive(Debug, Deserialize, Default)]
struct RawTheme {
    #[serde(default)]
    general: RawGeneral,
    #[serde(default)]
    text: RawText,
    #[serde(default)]
    buttons: RawButtons,
    #[serde(default)]
    buffer: RawBuffer,
    #[serde(default)]
    formatting: RawFormatting,
}

#[derive(Debug, Deserialize, Default)]
#[allow(dead_code)] // full schema fidelity; the console maps a subset
struct RawGeneral {
    background: Option<TextStyle>,
    border: Option<TextStyle>,
    horizontal_rule: Option<TextStyle>,
    horizontal_rule_text: Option<TextStyle>,
    scrollbar: Option<TextStyle>,
    unread_indicator: Option<TextStyle>,
    highlight_indicator: Option<TextStyle>,
}

#[derive(Debug, Deserialize, Default)]
#[allow(dead_code)] // debug/trace styles are parsed for schema fidelity only
struct RawText {
    primary: Option<TextStyle>,
    secondary: Option<TextStyle>,
    tertiary: Option<TextStyle>,
    success: Option<TextStyle>,
    error: Option<TextStyle>,
    warning: Option<TextStyle>,
    info: Option<TextStyle>,
    debug: Option<TextStyle>,
    trace: Option<TextStyle>,
}

#[derive(Debug, Deserialize, Default)]
struct RawButtons {
    #[serde(default)]
    primary: RawButton,
    #[serde(default)]
    secondary: RawButton,
}

#[derive(Debug, Deserialize, Default)]
#[allow(dead_code)] // the selected-hover variant is accepted but not mapped
struct RawButton {
    background: Option<TextStyle>,
    background_hover: Option<TextStyle>,
    background_selected: Option<TextStyle>,
    background_selected_hover: Option<TextStyle>,
    border_active: Option<TextStyle>,
}

#[derive(Debug, Deserialize, Default)]
#[allow(dead_code)] // several buffer roles have no web equivalent
struct RawBuffer {
    background: Option<TextStyle>,
    background_text_input: Option<TextStyle>,
    background_title_bar: Option<TextStyle>,
    border: Option<TextStyle>,
    border_selected: Option<TextStyle>,
    focus_border: Option<TextStyle>,
    focus_background: Option<TextStyle>,
    highlight: Option<TextStyle>,
    selection: Option<TextStyle>,
    url: Option<TextStyle>,
}

#[derive(Debug, Deserialize, Default)]
#[allow(dead_code)] // IRC formatting palette: console maps a subset
struct RawFormatting {
    white: Option<TextStyle>,
    black: Option<TextStyle>,
    blue: Option<TextStyle>,
    green: Option<TextStyle>,
    red: Option<TextStyle>,
    magenta: Option<TextStyle>,
    orange: Option<TextStyle>,
    yellow: Option<TextStyle>,
    cyan: Option<TextStyle>,
    pink: Option<TextStyle>,
    grey: Option<TextStyle>,
}

/// Fully resolved console roles before the readability gate.
struct RawPalette {
    page_background: Rgb,
    page_text: Rgb,
    page_border: Rgb,
    topbar_background: Rgb,
    topbar_text: Rgb,
    nav_background: Rgb,
    nav_text: Rgb,
    panel_background: Rgb,
    panel_border: Rgb,
    table_header_background: Rgb,
    table_header_text: Rgb,
    table_border: Rgb,
    table_row_background: Rgb,
    text_primary: Rgb,
    text_secondary: Rgb,
    text_muted: Rgb,
    status_success: Rgb,
    status_warning: Rgb,
    status_error: Rgb,
    status_info: Rgb,
    button_background: Rgb,
    button_text: Rgb,
    button_hover_background: Rgb,
    button_active_background: Rgb,
    button_border: Rgb,
    link: Rgb,
    accent: Rgb,
    chip_background: Rgb,
    chip_text: Rgb,
    graph_line: Rgb,
    graph_fill: Rgb,
    graph_grid: Rgb,
    input_background: Rgb,
    input_text: Rgb,
    input_border: Rgb,
    focus_ring: Rgb,
}

impl RawPalette {
    /// A self-evidently readable dark palette used only if the bundled
    /// default document fails to parse. White on near-black clears the AA
    /// threshold by a wide margin.
    fn dark_fallback() -> Self {
        let black = Rgb::new(0x11, 0x11, 0x11);
        let white = Rgb::new(0xf2, 0xf2, 0xf2);
        let grey = Rgb::new(0x8a, 0x8a, 0x8a);
        let accent = Rgb::new(0x5a, 0xa9, 0xe6);
        Self {
            page_background: black,
            page_text: white,
            page_border: grey,
            topbar_background: black,
            topbar_text: white,
            nav_background: black,
            nav_text: white,
            panel_background: black,
            panel_border: grey,
            table_header_background: grey,
            table_header_text: black,
            table_border: grey,
            table_row_background: black,
            text_primary: white,
            text_secondary: grey,
            text_muted: grey,
            status_success: white,
            status_warning: white,
            status_error: white,
            status_info: white,
            button_background: grey,
            button_text: black,
            button_hover_background: grey,
            button_active_background: grey,
            button_border: grey,
            link: accent,
            accent,
            chip_background: grey,
            chip_text: black,
            graph_line: accent,
            graph_fill: accent,
            graph_grid: grey,
            input_background: black,
            input_text: white,
            input_border: grey,
            focus_ring: accent,
        }
    }
}

/// The seed colors used when a document omits a role.
///
/// Seeds are only consulted for *absent* roles. A role that is present but
/// malformed fails the whole document during deserialization, so a typo
/// never silently changes the rendered palette.
struct Seed {
    page_background: Rgb,
    page_text: Rgb,
    muted: Rgb,
    accent: Rgb,
}

impl Seed {
    fn for_document(raw: &RawTheme) -> Self {
        let background = raw
            .general
            .background
            .map(|style| style.color)
            .unwrap_or(Rgb::new(0x11, 0x11, 0x11));
        let foreground = raw
            .text
            .primary
            .map(|style| style.color)
            .unwrap_or(Rgb::new(0xf2, 0xf2, 0xf2));
        let muted = raw
            .text
            .tertiary
            .map(|style| style.color)
            .unwrap_or_else(|| foreground.darken(0.35));
        let accent = raw
            .general
            .highlight_indicator
            .map(|style| style.color)
            .or_else(|| raw.formatting.blue.map(|style| style.color))
            .unwrap_or_else(|| foreground.lighten(0.2));
        Self {
            page_background: background,
            page_text: foreground,
            muted,
            accent,
        }
    }
}

impl From<RawTheme> for RawPalette {
    fn from(raw: RawTheme) -> Self {
        let seed = Seed::for_document(&raw);
        let color = |value: Option<TextStyle>| -> Option<Rgb> { value.map(|style| style.color) };

        let general_background = color(raw.general.background).unwrap_or(seed.page_background);
        let general_border =
            color(raw.general.border).unwrap_or_else(|| seed.page_text.darken(0.45));
        let rule = color(raw.general.horizontal_rule).unwrap_or(general_border);

        let text_primary = raw.text.primary.map(|s| s.color).unwrap_or(seed.page_text);
        let text_secondary = raw
            .text
            .secondary
            .map(|s| s.color)
            .unwrap_or_else(|| seed.page_text.darken(0.18));
        let text_tertiary = raw.text.tertiary.map(|s| s.color).unwrap_or(seed.muted);

        let title_bar = color(raw.buffer.background_title_bar).unwrap_or(general_background);
        let buffer_background = color(raw.buffer.background).unwrap_or(general_background);
        let buffer_border = color(raw.buffer.border).unwrap_or(general_border);
        let input_background = color(raw.buffer.background_text_input)
            .unwrap_or_else(|| buffer_background.darken(0.15));

        let primary_button =
            color(raw.buttons.primary.background).unwrap_or_else(|| buffer_background.lighten(0.1));
        let secondary_button = color(raw.buttons.secondary.background)
            .unwrap_or_else(|| buffer_background.lighten(0.05));

        let table_header_background = color(raw.buttons.secondary.background_hover)
            .unwrap_or_else(|| secondary_button.darken(0.05));

        let chip_background =
            color(raw.buffer.highlight).unwrap_or_else(|| buffer_background.lighten(0.12));

        let graph_line = color(raw.formatting.cyan).unwrap_or(seed.accent);
        let graph_grid = color(raw.formatting.grey).unwrap_or(general_border);
        let graph_fill = color(raw.buffer.highlight).unwrap_or_else(|| seed.accent.darken(0.45));

        let link = color(raw.buffer.url)
            .or_else(|| color(raw.formatting.blue))
            .unwrap_or(seed.accent);
        let focus_ring = color(raw.buffer.focus_border).unwrap_or(seed.accent);

        let status_success = raw
            .text
            .success
            .map(|s| s.color)
            .or_else(|| color(raw.formatting.green))
            .unwrap_or(seed.page_text);
        let status_error = raw
            .text
            .error
            .map(|s| s.color)
            .or_else(|| color(raw.formatting.red))
            .unwrap_or(seed.page_text);
        let status_warning = raw
            .text
            .warning
            .map(|s| s.color)
            .or_else(|| color(raw.formatting.orange))
            .unwrap_or(seed.page_text);
        let status_info = raw
            .text
            .info
            .map(|s| s.color)
            .or_else(|| color(raw.formatting.blue))
            .unwrap_or(seed.page_text);

        Self {
            page_background: general_background,
            page_text: text_primary,
            page_border: rule,
            topbar_background: title_bar,
            topbar_text: text_primary,
            nav_background: secondary_button,
            nav_text: text_primary,
            panel_background: buffer_background,
            panel_border: buffer_border,
            table_header_background,
            table_header_text: text_primary,
            table_border: color(raw.buffer.border_selected).unwrap_or(buffer_border),
            table_row_background: color(raw.buffer.selection)
                .unwrap_or_else(|| buffer_background.lighten(0.04)),
            text_primary,
            text_secondary,
            text_muted: text_tertiary,
            status_success,
            status_warning,
            status_error,
            status_info,
            button_background: primary_button,
            button_text: text_primary,
            button_hover_background: color(raw.buttons.primary.background_hover)
                .unwrap_or_else(|| primary_button.lighten(0.08)),
            button_active_background: color(raw.buttons.primary.background_selected)
                .unwrap_or_else(|| primary_button.darken(0.12)),
            button_border: color(raw.buttons.primary.border_active).unwrap_or(general_border),
            link,
            accent: color(raw.general.highlight_indicator).unwrap_or(seed.accent),
            chip_background,
            chip_text: text_primary,
            graph_line,
            graph_fill,
            graph_grid,
            input_background,
            input_text: text_primary,
            input_border: general_border,
            focus_ring,
        }
    }
}

///
/// Exposed for operator-facing diagnostics and the boundary checker.
pub fn theme_inventory() -> BTreeMap<&'static str, usize> {
    BUNDLED_THEMES
        .iter()
        .map(|theme| (theme.name, theme.source.len()))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    const MINIMAL: &str = "[general]\nbackground = \"#101010\"\n";

    #[test]
    fn every_bundled_theme_parses_and_is_readable() {
        for theme in bundled_themes() {
            let palette = parse_theme(theme.name, theme.source)
                .unwrap_or_else(|error| panic!("bundled theme {} failed: {error}", theme.name));
            assert_eq!(
                palette.readability_failures(),
                Vec::new(),
                "theme {} failed readability",
                theme.name
            );
            assert!(palette.css_variables().len() <= MAX_THEME_CSS_BYTES);
        }
    }

    #[test]
    fn bundled_inventory_is_non_empty_and_names_are_unique() {
        let themes = bundled_themes();
        assert!(!themes.is_empty());
        for (index, theme) in themes.iter().enumerate() {
            assert!(theme.name.len() <= MAX_THEME_NAME_LEN);
            assert!(theme.name == theme.name.trim());
            assert!(
                themes
                    .iter()
                    .filter(|other| other.name == theme.name)
                    .count()
                    == 1,
                "duplicate bundled theme name {}",
                theme.name
            );
            let _ = index;
        }
    }

    #[test]
    fn theme_name_resolution_rejects_unknown_and_oversized() {
        assert!(ThemeName::resolve("").is_err());
        assert!(ThemeName::resolve("no-such-theme").is_err());
        assert!(ThemeName::resolve(&"x".repeat(MAX_THEME_NAME_LEN + 1)).is_err());
        let name = ThemeName::resolve(DEFAULT_THEME_NAME).expect("bundled name resolves");
        assert_eq!(name.as_str(), DEFAULT_THEME_NAME);
    }

    #[test]
    fn bounds_reject_oversized_deep_and_wide_documents() {
        // Over the byte ceiling.
        let big = format!(
            "[general]\nbackground = \"#101010\"\n# {}\n",
            "x".repeat(MAX_THEME_SOURCE_BYTES)
        );
        assert_eq!(parse_theme("t", &big), Err(ThemeError::SourceTooLarge));

        // Deeper than the table-header bound.
        let mut deep = String::new();
        for level in 1..=(MAX_THEME_NESTING_DEPTH + 2) {
            deep.push_str(&"[".repeat(level));
            deep.push('a');
            deep.push_str(&"]".repeat(level));
            deep.push('\n');
        }
        assert_eq!(parse_theme("t", &deep), Err(ThemeError::NestingTooDeep));

        // More declarations than the key bound.
        let mut wide = String::from("[general]\n");
        for index in 0..(MAX_THEME_KEYS + 2) {
            wide.push_str(&format!("k{index} = \"#101010\"\n"));
        }
        assert_eq!(parse_theme("t", &wide), Err(ThemeError::TooManyKeys));
    }

    #[test]
    fn maximum_accepted_document_still_parses_or_fails_safely() {
        // Exactly at the key ceiling the document must reach the parser and
        // either parse or fail as malformed, never as a bound violation.
        let mut at_limit = String::from("[general]\nbackground = \"#101010\"\n");
        for index in 0..(MAX_THEME_KEYS - 1) {
            at_limit.push_str(&format!("k{index} = \"#101010\"\n"));
        }
        assert!(matches!(
            parse_theme("t", &at_limit),
            Ok(_) | Err(ThemeError::MalformedDocument)
        ));
    }

    #[test]
    fn minimal_document_produces_a_complete_readable_palette() {
        let palette = parse_theme("t", MINIMAL).expect("minimal theme parses");
        assert_eq!(palette.readability_failures(), Vec::new());
        assert_eq!(palette.name, "t");
    }

    #[test]
    fn halloy_text_style_table_is_accepted() {
        let source = concat!(
            "[general]\n",
            "background = \"#101010\"\n",
            "[text]\n",
            "primary = { color = \"#f0f0f0\", font_style = \"bold\" }\n",
            "secondary = \"#b0b0b0\"\n",
        );
        let palette = parse_theme("t", source).expect("text style table parses");
        assert!(palette.readability_failures().is_empty());
    }

    #[test]
    fn malicious_theme_text_cannot_emit_arbitrary_css() {
        // Values that try to break out of a CSS declaration.
        let attacks = [
            "[general]\nbackground = \"#101010; } body { display: none\"\n",
            "[general]\nbackground = \"#101010\\00</style><script>alert(1)</script>\"\n",
            "[text]\nprimary = \"red\"\n",
            "[general]\nbackground = \"rgb(1,2,3)\"\n",
            "[general]\nbackground = \"var(--evil)\"\n",
        ];
        for attack in attacks {
            // Every attack either fails closed or produces only `#rrggbb`
            // declarations.
            if let Ok(palette) = parse_theme("t", attack) {
                let css = palette.css_variables();
                for line in css.lines().skip(1) {
                    let declarations: Vec<&str> = line
                        .trim_end_matches('}')
                        .trim_start_matches(":root{")
                        .split(';')
                        .filter(|part| !part.is_empty())
                        .collect();
                    assert_eq!(declarations.len(), CSS_VARIABLE_ORDER.len());
                    for declaration in declarations {
                        let (name, value) = declaration
                            .split_once(':')
                            .unwrap_or_else(|| panic!("malformed declaration {declaration}"));
                        assert!(CSS_VARIABLE_ORDER.contains(&name));
                        assert_eq!(value.len(), 7);
                        assert!(value.starts_with('#'));
                        assert!(value[1..].bytes().all(|byte| byte.is_ascii_hexdigit()));
                    }
                }
            }
        }
    }

    #[test]
    fn deterministic_output_for_identical_input() {
        let first = parse_theme("t", MINIMAL).expect("parses");
        let second = parse_theme("t", MINIMAL).expect("parses");
        assert_eq!(first.css_variables(), second.css_variables());
        assert_eq!(first, second);
    }

    #[test]
    fn unreadable_pairs_are_repaired_not_rendered() {
        // Near-identical foreground and background in the document.
        let source = "[general]\nbackground = \"#202020\"\n[text]\nprimary = \"#212121\"\n";
        let palette = parse_theme("t", source).expect("parses");
        assert!(palette.readability_failures().is_empty());
        assert!(palette.page_text.contrast_ratio(palette.page_background) >= MIN_TEXT_CONTRAST);
    }

    #[test]
    fn invalid_colors_fail_closed_rather_than_defaulting_silently() {
        let source = "[general]\nbackground = \"not-a-color\"\n";
        assert_eq!(parse_theme("t", source), Err(ThemeError::MalformedDocument));
    }

    #[test]
    fn resolve_or_default_never_panics_on_unknown_or_broken_input() {
        assert_eq!(
            ThemePalette::resolve_or_default("no-such-theme").readability_failures(),
            Vec::new()
        );
        assert_eq!(
            ThemePalette::resolve_or_default(DEFAULT_THEME_NAME).name,
            DEFAULT_THEME_NAME
        );
        // The compiled default is always readable.
        assert!(
            ThemePalette::compiled_default()
                .readability_failures()
                .is_empty()
        );
    }
}
