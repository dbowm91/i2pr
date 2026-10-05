# Console theme provenance

The console implements the **Halloy theme schema** as data. It does not
vendor Halloy's palettes, CSS, or templates.

## What is upstream

The schema — the table names `[general]`, `[text]`, `[buttons.*]`,
`[buffer]`, `[buffer.server_messages]`, `[formatting]`, the accepted
`font_style` vocabulary, and the `#rrggbb` color grammar — is taken from the
published Halloy custom-theme documentation. Schema knowledge is not a
copyable asset; the parser in `src/theme.rs` is an independent
implementation bounded by this repository's own limits
(`MAX_THEME_SOURCE_BYTES`, `MAX_THEME_NESTING_DEPTH`, `MAX_THEME_KEYS`,
`crate::color::MAX_COLOR_LITERAL_LEN`).

Upstream Halloy source is GPL-licensed. **No Halloy file is copied into this
repository**, so no GPL obligation attaches to the console assets.

## What is original

These three palettes were authored for i2pr and are covered by this
repository's own license:

| Theme name            | File                        | Character            |
| --------------------- | --------------------------- | -------------------- |
| `i2pr-default`        | `i2pr-default.toml`         | dark, neutral blue    |
| `i2pr-midnight`       | `i2pr-midnight.toml`        | darker, lower contrast accents |
| `i2pr-daylight`       | `i2pr-daylight.toml`        | light background      |

Each is validated at build/test time by
`every_bundled_theme_parses_and_is_readable`, which asserts the whole
palette clears the console's core-pairing readability floor.

## Not vendored

The roadmap milestone anticipated importing a 50-theme set from the
EggPool/Halloy ecosystem. That import is **not performed here**: it was not
possible to establish a redistribution license for those specific palette
files, and Plan 356 requires provenance-clean assets or the import is
recorded as blocked follow-up. The Halloy-compatible parser and adapter are
complete, so adding a licensed theme set later is a data change, not a code
change.

## Adding a theme

1. Author an original palette in the schema above, or add a file whose
   license has been reviewed and recorded here.
2. Register it in `BUNDLED_THEMES` in `src/theme.rs`.
3. Run `cargo test --locked -p i2pr-console`. The inventory-uniqueness,
   parse, and readability tests must pass.
4. Record the license and provenance in this file in the same commit.

Never add a theme whose provenance cannot be stated.