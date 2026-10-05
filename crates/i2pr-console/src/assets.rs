//! The compile-time asset table.
//!
//! Every browser-reachable asset is listed here as a `(path, body,
//! content-type)` triple compiled into the binary. There is no filesystem
//! lookup, no directory walk, and no request-driven path join, so a
//! request cannot name a resource that is not in this table and cannot
//! escape a static root.

/// Maximum accepted size of one served asset.
///
/// The table is compile-time fixed, so this is a regression tripwire that
/// fails a build rather than a runtime rejection.
pub const MAX_ASSET_BYTES: usize = 64 * 1024;

/// A compile-time browser asset.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Asset {
    /// Stable request path under `/assets/`.
    pub path: &'static str,
    /// Complete file body, compiled in.
    pub body: &'static str,
    /// Value of the `Content-Type` response header.
    pub content_type: &'static str,
}

/// The complete asset table.
static ASSETS: &[Asset] = &[
    Asset {
        path: "console.css",
        body: include_str!("../assets/console.css"),
        content_type: "text/css; charset=utf-8",
    },
    Asset {
        path: "console.js",
        body: include_str!("../assets/console.js"),
        content_type: "text/javascript; charset=utf-8",
    },
    Asset {
        path: "logo.svg",
        body: include_str!("../assets/logo.svg"),
        content_type: "image/svg+xml",
    },
];

/// Returns the full asset table in declaration order.
pub fn assets() -> &'static [Asset] {
    ASSETS
}

/// Looks up an asset by its exact table entry name.
///
/// Match is exact and case-sensitive against the compiled table; there is
/// no normalization, no percent-decoding fallback, and no prefix matching.
pub fn find(name: &str) -> Option<&'static Asset> {
    ASSETS.iter().find(|asset| asset.path == name)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn table_is_non_empty_and_bounded() {
        assert!(!ASSETS.is_empty());
        for asset in ASSETS {
            assert!(
                asset.body.len() <= MAX_ASSET_BYTES,
                "asset {} exceeds the ceiling",
                asset.path
            );
        }
    }

    #[test]
    fn table_paths_are_unique_and_have_no_traversal_shapes() {
        for (index, asset) in ASSETS.iter().enumerate() {
            assert_eq!(
                ASSETS
                    .iter()
                    .filter(|other| other.path == asset.path)
                    .count(),
                1,
                "duplicate asset path {}",
                asset.path
            );
            assert!(!asset.path.is_empty());
            assert!(!asset.path.contains('/'), "table keys are flat names");
            assert!(!asset.path.contains('\\'));
            assert!(!asset.path.contains(".."));
            assert!(!asset.path.starts_with('.'));
            let _ = index;
        }
    }

    #[test]
    fn every_asset_declares_a_known_content_type() {
        let allowed = [
            "text/css; charset=utf-8",
            "text/javascript; charset=utf-8",
            "image/svg+xml",
        ];
        for asset in ASSETS {
            assert!(
                allowed.contains(&asset.content_type),
                "unexpected content type for {}",
                asset.path
            );
        }
    }

    #[test]
    fn lookup_is_exact_and_never_traverses() {
        assert!(find("console.css").is_some());
        // No normalization, no prefix match, no traversal.
        for hostile in [
            "CONSOLE.CSS",
            "./console.css",
            "../console.css",
            "console.css/",
            "subdir/console.css",
            "%2e%2e/console.css",
            "",
            "..",
        ] {
            assert_eq!(find(hostile), None, "lookup accepted {hostile}");
        }
    }

    #[test]
    fn shipped_assets_are_csp_clean() {
        for asset in ASSETS {
            if asset.content_type.starts_with("text/javascript") {
                assert!(
                    !asset.body.contains("eval("),
                    "{} must not use eval",
                    asset.path
                );
                assert!(
                    !asset.body.contains("new Function"),
                    "{} must not construct functions from strings",
                    asset.path
                );
                assert!(
                    !asset.body.contains("http://") && !asset.body.contains("https://"),
                    "{} must not reference a remote origin",
                    asset.path
                );
            }
        }
    }
}
