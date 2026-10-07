//! Plan 380: the i2pr **typed extension seam** on the Proposal 170 `CustomOptions` field.
//!
//! # Why this exists
//!
//! Every service-tunnel option i2pr accepts today reaches the daemon because Proposal 170
//! *defines a wire field for it*: `leaseset_password` ← `OptionalLookup`, `leaseset_client_auth`
//! ← `LeaseSetClientAuths`, the Plan 342 outproxy block ← `ProxyList` / `Outproxy*`. The parser
//! admits a top-level field only when `proposal_wire::proposal_tunnel_value_type` recognises it,
//! and that function is gated on the frozen `PROPOSAL_TUNNEL_MANAGER_FIELDS` inventory.
//!
//! Plan 380 needs a consumer client credential — the PSK or DH key a `.b33` publisher authorised
//! this router to use. Proposal 170 has no such field, and there is nothing to map it onto. So
//! either the frozen inventory gains a name Proposal does not define, or i2pr needs a seam of its
//! own. This is the seam, and it is deliberately built so that the first option is not taken.
//!
//! # What it is not
//!
//! It is **not** an escape hatch. `CustomOptions` in Proposal 170 is the I2CP-style untyped
//! pass-through, which is why this repository refused it outright: an arbitrary `Key=Value` blob
//! reaching an I2CP adapter is an injection surface, and the refusal named that precisely —
//! "no safe typed allowlist".
//!
//! That refusal stands. The untyped form — any value that is not a JSON object — is still
//! rejected with exactly the same error it always was, and the existing rows that prove it stay
//! green and unchanged.
//!
//! What is added is the narrow other side of that refusal: an object with exactly one namespace
//! key, holding a **closed allowlist** of typed entries. There is no free-form key, no nested
//! namespace, and no value that is not a string. An unrecognised key is refused rather than
//! ignored, because silently dropping a setting an operator typed is how a router ends up
//! configured in a posture nobody chose.
//!
//! # Why a namespace at all
//!
//! Because the field is Proposal 170's. Anything carried in it that did not read as an i2pr
//! extension would be indistinguishable from a Proposal value, and an operator reading a
//! transcript could not tell which half of the request i2pr honoured. `{"i2pr": {...}}` is
//! self-describing: a stock implementation that does not know this shape ignores or rejects the
//! whole field, and never half-applies it.
//!
//! # Bounds
//!
//! One namespace, at most [`MAX_EXTENSION_ENTRIES`] entries, each key at most
//! [`MAX_EXTENSION_NAME_LEN`] bytes, each value at most [`MAX_EXTENSION_VALUE_LEN`]. The decoded
//! entries become ordinary entries in the same bounded option map every Proposal option lands in,
//! so the existing `MAX_OPTIONS_PER_TUNNEL` ceiling applies to them too — an extension cannot be
//! used to smuggle options past a limit that applies to everything else.
//!
//! # Grammar is not enforced here
//!
//! This layer checks *shape*: an object, one namespace, a known key, a string, bounded. Whether
//! the string is a well-formed credential is the credential owner's business — the daemon's
//! `EncryptedTargetCredential::parse` refuses a malformed one, and duplicating that grammar here
//! would create two rules that can disagree. The value is carried as a plain string and reaches
//! no status surface, log line, or error message before the daemon seals it.

use serde_json::Value;

/// The one namespace key an extension object may carry.
pub const EXTENSION_NAMESPACE: &str = "i2pr";

/// Maximum entries in one extension namespace.
pub const MAX_EXTENSION_ENTRIES: usize = 4;

/// Maximum length of one extension entry's name.
pub const MAX_EXTENSION_NAME_LEN: usize = 64;

/// Maximum length of one extension entry's value.
///
/// Deliberately the same ceiling as a Proposal option value. An extension entry becomes an
/// ordinary option entry, so holding it to a different ceiling would be a way around the one
/// bound that governs everything else on this surface.
pub const MAX_EXTENSION_VALUE_LEN: usize = crate::limits::MAX_OPTION_VALUE_LEN;

/// The closed allowlist: wire name → internal option name.
///
/// A `&'static str` on both sides, so neither the wire name nor the internal name can be
/// attacker-controlled text and neither can be used as a lookup key at all. Adding an entry here
/// is the whole of "what i2pr extensions exist".
const EXTENSION_ALLOWLIST: [(&str, &str); 1] = [(
    "LeasesetClientCredential",
    LEASE_SET_CLIENT_CREDENTIAL_OPTION,
)];

/// Plan 380: the internal option name for the ELS2 consumer client credential.
///
/// Exported so the daemon can name the same constant instead of repeating the literal. The Plan
/// 342 outproxy block does this the same way — `SUPPORTED_342_OPTIONS` is built from
/// `outproxy_options::OUTPROXY_BLOCK_KEYS` rather than restating the names — and the reason is
/// the same: two copies of a wire-to-internal mapping are two chances to disagree, and a
/// disagreement here is an option that parses and is then silently refused later.
pub const LEASE_SET_CLIENT_CREDENTIAL_OPTION: &str = "leaseset_client_credential";

/// Why an extension object was refused.
///
/// Every variant is a literal. The offending key is reported only where it is bounded and
/// non-secret by construction — see [`Self::UnknownKey`]. Values never appear: an extension value
/// is secret material by definition of this seam, and an error string that can carry one would
/// put it into a client reply.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ExtensionError {
    /// The value was not a JSON object — the untyped Proposal pass-through form.
    ///
    /// Distinct from [`Self::NamespaceNotAnObject`] on purpose. The caller reports this one as
    /// the historical `CustomOptions` refusal, because it *is* that case: a Proposal-shaped blob
    /// with no typed owner. Conflating the two would report a malformed extension as an untyped
    /// one and send an operator looking for an I2CP adapter problem they do not have.
    Untyped,
    /// The namespace was present and correctly named but its value was not an object.
    NamespaceNotAnObject,
    /// The object did not carry exactly the one namespace key.
    BadNamespace,
    /// More entries than [`MAX_EXTENSION_ENTRIES`].
    TooManyEntries,
    /// An entry name was over bound, or empty.
    NameOverBound,
    /// An entry value was over bound.
    ValueOverBound,
    /// An entry value was present but was not a string.
    NotAString,
    /// An entry name that is not in the closed allowlist.
    ///
    /// Carries the name because it is bounded by [`MAX_EXTENSION_NAME_LEN`] before this is
    /// constructed, and because an unknown key is a client mistake whose name is the whole of the
    /// actionable part of the answer. It is not secret: an attacker-supplied name is not.
    UnknownKey,
}

impl core::fmt::Display for ExtensionError {
    fn fmt(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        let reason = match self {
            Self::Untyped => "must be an object of typed extensions, not an untyped value",
            Self::NamespaceNotAnObject => "extension namespace must hold an object",
            Self::BadNamespace => "must carry exactly one namespace key named \"i2pr\"",
            Self::TooManyEntries => "carries more extension entries than the ceiling allows",
            Self::NameOverBound => "extension name must be non-empty and bounded",
            Self::ValueOverBound => "extension value is over the option ceiling",
            Self::NotAString => "extension value must be a string",
            Self::UnknownKey => "extension has no safe typed allowlist",
        };
        formatter.write_str(reason)
    }
}

impl std::error::Error for ExtensionError {}

/// Decodes one `CustomOptions` extension object into internal option entries.
///
/// Returns `(internal_option_name, value)` pairs in the allowlist's order. An empty namespace
/// object decodes to no entries, which is not an error: a caller that constructs the object
/// conditionally should not have to remove the field to send a legal request.
///
/// A non-object is [`ExtensionError::Untyped`], which the caller reports as the historical
/// `CustomOptions` refusal. That distinction is the point of this module — the untyped form is
/// still rejected, and for the same reason it always was.
pub fn decode_extension_options(
    value: &Value,
) -> Result<Vec<(&'static str, String)>, ExtensionError> {
    let Some(outer) = value.as_object() else {
        return Err(ExtensionError::Untyped);
    };
    // Exactly one key, and it must be the namespace. A second key is refused rather than
    // ignored: `{"i2pr": {...}, "other": {...}}` means the sender expected two vocabularies, and
    // honouring one of them silently is the outcome this seam exists to prevent.
    if outer.len() != 1 {
        return Err(ExtensionError::BadNamespace);
    }
    let Some((namespace, inner)) = outer.iter().next() else {
        return Err(ExtensionError::BadNamespace);
    };
    if namespace != EXTENSION_NAMESPACE {
        return Err(ExtensionError::BadNamespace);
    }
    let Some(entries) = inner.as_object() else {
        return Err(ExtensionError::NamespaceNotAnObject);
    };
    if entries.len() > MAX_EXTENSION_ENTRIES {
        return Err(ExtensionError::TooManyEntries);
    }
    let mut decoded = Vec::with_capacity(entries.len());
    for (name, entry) in entries {
        if name.is_empty() || name.len() > MAX_EXTENSION_NAME_LEN {
            return Err(ExtensionError::NameOverBound);
        }
        // Fail closed on an unrecognised name. The name itself is not echoed into the error —
        // the reason text names the rule — so this cannot become a channel for smuggling bytes
        // back to the client that sent them.
        let Some((_, internal)) = EXTENSION_ALLOWLIST
            .iter()
            .find(|(wire, _)| *wire == name.as_str())
        else {
            return Err(ExtensionError::UnknownKey);
        };
        let Some(text) = entry.as_str() else {
            return Err(ExtensionError::NotAString);
        };
        if text.len() > MAX_EXTENSION_VALUE_LEN {
            return Err(ExtensionError::ValueOverBound);
        }
        decoded.push((*internal, text.to_owned()));
    }
    Ok(decoded)
}

/// The wire names this seam accepts, for a caller that needs to report the surface.
///
/// Exists so a "which extensions does this build support?" question has one answer that cannot
/// drift from the allowlist, rather than a second list to keep in step.
pub fn supported_extension_names() -> impl Iterator<Item = &'static str> {
    EXTENSION_ALLOWLIST.iter().map(|(wire, _)| *wire)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn the_els2_credential_decodes_to_its_internal_name() {
        let decoded = decode_extension_options(&json!({
            "i2pr": {"LeasesetClientCredential": "psk:abc"}
        }))
        .expect("decodes");
        assert_eq!(
            decoded,
            vec![(LEASE_SET_CLIENT_CREDENTIAL_OPTION, "psk:abc".to_owned())]
        );
    }

    #[test]
    fn an_empty_namespace_is_legal_and_yields_nothing() {
        assert_eq!(
            decode_extension_options(&json!({"i2pr": {}})).expect("legal"),
            Vec::new()
        );
    }

    #[test]
    fn the_untyped_pass_through_form_is_still_refused() {
        // This is the case the repository refused before Plan 380, and the reason it refused is
        // unchanged: there is no typed allowlist for an I2CP blob.
        assert_eq!(
            decode_extension_options(&json!("TargetHost=attacker.invalid")),
            Err(ExtensionError::Untyped)
        );
        assert_eq!(
            decode_extension_options(&json!(["a", "b"])),
            Err(ExtensionError::Untyped)
        );
    }

    #[test]
    fn the_namespace_is_exactly_one_key_and_nothing_else() {
        assert_eq!(
            decode_extension_options(&json!({})),
            Err(ExtensionError::BadNamespace)
        );
        assert_eq!(
            decode_extension_options(&json!({"i2pr": {}, "other": {}})),
            Err(ExtensionError::BadNamespace)
        );
        assert_eq!(
            decode_extension_options(&json!({"I2PR": {}})),
            Err(ExtensionError::BadNamespace),
            "the namespace is case-sensitive: one spelling, not a family"
        );
        assert_eq!(
            decode_extension_options(&json!({"i2pr": "not-an-object"})),
            Err(ExtensionError::NamespaceNotAnObject)
        );
    }

    #[test]
    fn an_unknown_extension_name_is_refused_and_never_ignored() {
        // Silently dropping an unrecognised entry is how a router ends up configured in a
        // posture nobody chose, so this must be an error and not an empty decode.
        assert_eq!(
            decode_extension_options(&json!({"i2pr": {"NoSuchThing": "x"}})),
            Err(ExtensionError::UnknownKey)
        );
        assert_eq!(
            decode_extension_options(&json!({
                "i2pr": {"LeasesetClientCredential": "psk:abc", "NoSuchThing": "x"}
            })),
            Err(ExtensionError::UnknownKey)
        );
    }

    #[test]
    fn entries_are_bounded_in_count_name_length_and_value_length() {
        let mut too_many = serde_json::Map::new();
        for index in 0..=MAX_EXTENSION_ENTRIES {
            too_many.insert(format!("k{index}"), json!("v"));
        }
        assert_eq!(
            decode_extension_options(&json!({"i2pr": Value::Object(too_many)})),
            Err(ExtensionError::TooManyEntries)
        );

        assert_eq!(
            decode_extension_options(&json!({
                "i2pr": {"": "v"}
            })),
            Err(ExtensionError::NameOverBound)
        );
        assert_eq!(
            decode_extension_options(&json!({
                "i2pr": {format!("{}", "k".repeat(MAX_EXTENSION_NAME_LEN + 1)): "v"}
            })),
            Err(ExtensionError::NameOverBound)
        );

        let over = "v".repeat(MAX_EXTENSION_VALUE_LEN + 1);
        assert_eq!(
            decode_extension_options(&json!({"i2pr": {"LeasesetClientCredential": over}})),
            Err(ExtensionError::ValueOverBound)
        );
    }

    #[test]
    fn an_extension_value_must_be_a_string() {
        for shape in [json!(1), json!(true), json!(["psk:abc"]), json!({"a": "b"})] {
            assert_eq!(
                decode_extension_options(&json!({"i2pr": {"LeasesetClientCredential": shape}})),
                Err(ExtensionError::NotAString),
                "must refuse {shape}"
            );
        }
    }

    #[test]
    fn no_error_message_can_carry_an_extension_value() {
        // Every reason is a literal, so the `Display` of every variant is inspectable here
        // without any input that could contain a secret.
        for error in [
            ExtensionError::Untyped,
            ExtensionError::NamespaceNotAnObject,
            ExtensionError::BadNamespace,
            ExtensionError::TooManyEntries,
            ExtensionError::NameOverBound,
            ExtensionError::ValueOverBound,
            ExtensionError::NotAString,
            ExtensionError::UnknownKey,
        ] {
            let rendered = error.to_string();
            assert!(!rendered.is_empty());
            assert!(
                !rendered.contains("psk:") && !rendered.contains("dh:"),
                "a reason must never contain a credential: {rendered}"
            );
        }
    }

    #[test]
    fn the_allowlist_is_the_whole_of_the_supported_surface() {
        let names: Vec<&str> = supported_extension_names().collect();
        assert_eq!(
            names,
            vec!["LeasesetClientCredential"],
            "this row is what forces a new extension to be a deliberate act"
        );
    }
}
