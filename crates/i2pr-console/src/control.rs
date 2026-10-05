//! The console's read-only Proposal-170 boundary.
//!
//! The console never speaks the control wire and never names a control
//! method. It depends on [`ControlClient`], a two-method trait whose
//! implementations are supplied by the daemon and hold the local console
//! principal. Because the trait surface is closed, a browser request
//! cannot reach a method the trait does not expose, whatever it sends.
//!
//! Everything crossing this boundary is already bounded and redacted by
//! the canonical control dispatch. This module adds presentation semantics
//! only, and it is explicit about the one thing a presentation layer must
//! never do: turn an unavailable answer into a zero.

use std::fmt;

use serde_json::Value;

/// Maximum number of rows the overview may carry.
pub const MAX_OVERVIEW_ROWS: usize = 64;

/// Maximum length of a single presented value.
pub const MAX_VALUE_LEN: usize = 256;

/// Maximum number of service rows presented at once.
pub const MAX_SERVICE_ROWS: usize = 32;

/// How much the console may claim about one control answer.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Availability {
    /// The method is supported and returned a value.
    Returned,
    /// The method is known but its source is currently unavailable or
    /// publish-gated.
    Unavailable,
    /// The method or selector is not implemented by this revision.
    Unsupported,
    /// The control request itself failed.
    Failed,
}

impl Availability {
    /// Returns the stable wire token for this state.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Returned => "returned",
            Self::Unavailable => "unavailable",
            Self::Unsupported => "unsupported",
            Self::Failed => "failed",
        }
    }

    /// Returns whether a value exists for this state.
    pub const fn has_value(self) -> bool {
        matches!(self, Self::Returned)
    }
}

impl fmt::Display for Availability {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

/// One canonical control answer, already bounded by the control layer.
#[derive(Clone, Debug)]
pub struct ControlReply {
    /// What the console may claim about this answer.
    pub availability: Availability,
    /// The canonical result object, or an empty object when unavailable.
    pub value: Value,
    /// Short operator-facing detail, present only when not returned.
    pub detail: Option<String>,
}

impl ControlReply {
    /// Builds a reply with no value.
    pub fn unavailable(detail: impl Into<String>) -> Self {
        Self {
            availability: Availability::Unavailable,
            value: Value::Object(serde_json::Map::new()),
            detail: Some(truncate(detail.into())),
        }
    }

    /// Builds a failed reply with no value.
    pub fn failed(detail: impl Into<String>) -> Self {
        Self {
            availability: Availability::Failed,
            value: Value::Object(serde_json::Map::new()),
            detail: Some(truncate(detail.into())),
        }
    }

    /// Builds a returned reply.
    pub fn returned(value: Value) -> Self {
        Self {
            availability: Availability::Returned,
            value,
            detail: None,
        }
    }

    /// Reads one canonical field as a presented string.
    ///
    /// Returns `None` when the field is absent or unpresentable. A missing
    /// field is never rendered as zero, empty, or "false".
    pub fn field(&self, key: &str) -> Option<String> {
        if !self.availability.has_value() {
            return None;
        }
        let raw = self.value.get(key)?;
        present_value(raw)
    }

    /// Reads one canonical field as a bounded integer.
    pub fn integer(&self, key: &str) -> Option<i64> {
        if !self.availability.has_value() {
            return None;
        }
        match self.value.get(key)? {
            Value::Number(number) => number.as_i64(),
            Value::String(text) => text.parse().ok(),
            _ => None,
        }
    }
}

/// Clamps one JSON value to a presentable, bounded string.
fn present_value(raw: &Value) -> Option<String> {
    let rendered = match raw {
        Value::Null => return None,
        Value::Bool(flag) => flag.to_string(),
        Value::Number(number) => number.to_string(),
        Value::String(text) => text.clone(),
        Value::Array(items) => {
            let rendered: Vec<String> = items
                .iter()
                .take(MAX_SERVICE_ROWS)
                .map(|item| item.as_str().map(str::to_string).unwrap_or_default())
                .collect();
            rendered.join(", ")
        }
        // An object is structure, not a presentation value. Rendering it
        // would mean inventing a summary the control layer never produced.
        Value::Object(_) => return None,
    };
    let trimmed = truncate(rendered);
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed)
    }
}

/// Renders a service result object as bounded `key=value` pairs.
///
/// A client-service result is a small object (`{"enabled": false,
/// "port": 7656}`), not a scalar. Rendering it would mean inventing a
/// summary the control layer never produced, so this walks the object's
/// scalar leaves in sorted key order and stops at the first bound. An
/// object with no scalar leaf yields `None`, which the caller reports as
/// unavailable rather than as an empty "returned" row.
fn present_object_summary(raw: &Value) -> Option<String> {
    let map = raw.as_object()?;
    let mut parts: Vec<String> = Vec::new();
    for (key, value) in map {
        let rendered = match value {
            Value::Null => continue,
            Value::Bool(flag) => flag.to_string(),
            Value::Number(number) => number.to_string(),
            Value::String(text) => text.clone(),
            // Nested objects and arrays are structure; they are not
            // flattened, so the row shows only what it can state plainly.
            _ => continue,
        };
        parts.push(format!("{key}={rendered}"));
        if parts.len() >= MAX_SERVICE_FIELDS {
            break;
        }
    }
    if parts.is_empty() {
        return None;
    }
    parts.sort();
    Some(truncate(parts.join(", ")))
}

/// Maximum scalar leaves rendered for one service row.
pub const MAX_SERVICE_FIELDS: usize = 8;

/// Bounds a string to [`MAX_VALUE_LEN`] without splitting a UTF-8 code
/// point.
fn truncate(mut value: String) -> String {
    if value.chars().count() > MAX_VALUE_LEN {
        value = value.chars().take(MAX_VALUE_LEN).collect();
    }
    value
}

/// The console's read-only control surface.
///
/// Implementations are owned by the daemon and hold the local console
/// principal. There is no method-naming parameter anywhere in this trait,
/// which is what makes invariant "a browser cannot select arbitrary daemon
/// methods" a property of the type rather than a runtime check.
pub trait ControlClient: Send + Sync + fmt::Debug {
    /// Returns the canonical `RouterInfo` summary for the overview.
    fn router_info(&self) -> ControlReply;

    /// Returns the canonical `ClientServicesInfo` rows for the overview.
    fn client_services(&self) -> ControlReply;
}

/// A console with no control client installed.
///
/// It reports every field as unavailable rather than showing zeros, which
/// is the honest answer for a router whose control sources are not
/// composed.
#[derive(Clone, Copy, Debug, Default)]
pub struct UnavailableControlClient;

impl ControlClient for UnavailableControlClient {
    fn router_info(&self) -> ControlReply {
        ControlReply::unavailable("control sources are not composed")
    }

    fn client_services(&self) -> ControlReply {
        ControlReply::unavailable("control sources are not composed")
    }
}

/// One overview metric.
#[derive(Clone, Debug)]
pub struct OverviewMetric {
    /// Human label, already escaped at render time.
    pub label: &'static str,
    /// Presented value, absent when the source is unavailable.
    pub value: Option<String>,
    /// What the console may claim about this value.
    pub availability: Availability,
}

/// One client-service row.
#[derive(Clone, Debug)]
pub struct ServiceRow {
    /// Canonical service name.
    pub name: String,
    /// Presented state.
    pub state: Option<String>,
    /// What the console may claim about this row.
    pub availability: Availability,
}

/// The bounded overview view model.
#[derive(Clone, Debug)]
pub struct Overview {
    /// Summary metrics.
    pub metrics: Vec<OverviewMetric>,
    /// Client-service rows.
    pub services: Vec<ServiceRow>,
    /// Availability of the `RouterInfo` answer the metrics came from.
    pub router_info_availability: Availability,
    /// Availability of the `ClientServicesInfo` answer.
    pub services_availability: Availability,
}

/// Canonical `RouterInfo` selectors the overview requests.
///
/// This list is a compile-time constant. The console cannot ask for a
/// field the contract does not define, and it never asks for a mutating
/// selector.
pub const OVERVIEW_SELECTORS: &[&str] = &[
    "i2p.router.version",
    "i2p.router.uptime",
    "i2p.router.status",
    "i2p.router.net.status",
    "i2p.router.netdb.knownpeers",
    "i2p.router.netdb.activepeers",
];

/// Canonical `ClientServicesInfo` selectors the overview requests.
///
/// Like the RouterInfo list this is a compile-time constant: the console
/// cannot ask for a service the contract does not define.
pub const CLIENT_SERVICE_SELECTORS: &[&str] =
    &["I2PTunnel", "HTTPProxy", "SOCKS", "SAM", "BOB", "I2CP"];

/// Metrics presented in the overview, in order.
const METRICS: &[(&str, &str)] = &[
    ("i2p.router.version", "Version"),
    ("i2p.router.uptime", "Uptime"),
    ("i2p.router.status", "Status"),
    ("i2p.router.net.status", "Network status"),
    ("i2p.router.netdb.knownpeers", "Known peers"),
    ("i2p.router.netdb.activepeers", "Active peers"),
];

impl Overview {
    /// Builds the view model from the two canonical answers.
    ///
    /// A missing field produces a metric with no value and an explicit
    /// availability, never a zero.
    pub fn build(router_info: &ControlReply, services: &ControlReply) -> Self {
        let metrics = METRICS
            .iter()
            .take(MAX_OVERVIEW_ROWS)
            .map(|(key, label)| {
                let value = router_info.field(key);
                // A presented value is only claimable when the reply was
                // returned; otherwise the reply's own state governs.
                let availability = match (&value, router_info.availability) {
                    (Some(_), Availability::Returned) => Availability::Returned,
                    // A successful reply that did not carry the field means
                    // the source did not publish it: unavailable, not zero.
                    (None, Availability::Returned) => Availability::Unavailable,
                    (_, other) => other,
                };
                OverviewMetric {
                    label,
                    value,
                    availability,
                }
            })
            .collect();

        let reply = services;
        let rows = reply.value.as_object().map(|map| {
            map.iter()
                .filter(|(name, _)| name.as_str() != "i2p.router.status")
                .take(MAX_SERVICE_ROWS)
                .map(|(name, value)| {
                    let state = present_value(value).or_else(|| present_object_summary(value));
                    let availability = match (&state, services.availability) {
                        (Some(_), Availability::Returned) => Availability::Returned,
                        // A returned reply whose service object exposes no
                        // scalar leaf states nothing about that service.
                        // Reporting `Returned` here would claim the control
                        // plane answered when it rendered no value at all.
                        (None, Availability::Returned) => Availability::Unavailable,
                        (_, other) => other,
                    };
                    ServiceRow {
                        name: truncate(name.clone()),
                        state,
                        availability,
                    }
                })
                .collect()
        });
        let services: Vec<ServiceRow> = rows.unwrap_or_default();
        // A reply that carried no usable rows is reported at the reply's
        // own availability, not upgraded to "returned" just because the
        // render step produced an empty list.
        let services_availability = if !reply.availability.has_value() && services.is_empty() {
            reply.availability
        } else {
            Availability::Returned
        };

        Self {
            metrics,
            services,
            router_info_availability: router_info.availability,
            services_availability,
        }
    }

    /// Renders the view model as the bounded overview JSON document.
    pub fn to_json(&self) -> Value {
        let metrics: Vec<Value> = self
            .metrics
            .iter()
            .map(|metric| {
                serde_json::json!({
                    "label": metric.label,
                    "value": metric.value,
                    "availability": metric.availability.as_str(),
                })
            })
            .collect();
        let services: Vec<Value> = self
            .services
            .iter()
            .map(|row| {
                serde_json::json!({
                    "name": row.name,
                    "state": row.state,
                    "availability": row.availability.as_str(),
                })
            })
            .collect();
        serde_json::json!({
            "routerInfo": self.router_info_availability.as_str(),
            "clientServices": self.services_availability.as_str(),
            "metrics": metrics,
            "services": services,
        })
    }

    /// Returns whether the overview has any presented value.
    pub fn is_empty(&self) -> bool {
        self.metrics.iter().all(|metric| metric.value.is_none()) && self.services.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn reply(value: Value) -> ControlReply {
        ControlReply::returned(value)
    }

    #[test]
    fn selectors_are_a_closed_read_only_list() {
        // No mutating or capability-mutating selector may appear.
        for selector in OVERVIEW_SELECTORS {
            assert!(
                !selector.contains("logs.clear"),
                "{selector} is a mutation and must not be requested"
            );
            assert!(selector.starts_with("i2p.router."));
        }
        assert!(OVERVIEW_SELECTORS.contains(&"i2p.router.version"));
        // Every presented metric must come from a requested selector: a
        // label with no selector would silently render nothing.
        for selector in CLIENT_SERVICE_SELECTORS {
            assert!(
                selector
                    .chars()
                    .next()
                    .is_some_and(|first| first.is_ascii_uppercase()),
                "{selector} is not a canonical service spelling"
            );
        }
        for (key, _label) in METRICS {
            assert!(
                OVERVIEW_SELECTORS.contains(key),
                "{key} is presented but never requested"
            );
        }
        // Logs are a mutation and must never enter the overview.
        assert!(!OVERVIEW_SELECTORS.iter().any(|s| s.contains("logs")));
    }

    #[test]
    fn a_missing_field_is_unavailable_and_never_zero() {
        let control = reply(json!({ "i2p.router.version": "0.1.0" }));
        let overview = Overview::build(&control, &ControlReply::unavailable("no services"));
        assert_eq!(overview.metrics[0].value.as_deref(), Some("0.1.0"));
        assert_eq!(overview.metrics[0].availability, Availability::Returned);

        // Every other metric has no value rather than a fabricated zero.
        let uptime = overview
            .metrics
            .iter()
            .find(|metric| metric.label == "Uptime")
            .expect("uptime metric exists");
        assert_eq!(uptime.value, None);
        assert_ne!(uptime.availability, Availability::Returned);
        assert_ne!(uptime.value.as_deref(), Some("0"));
    }

    #[test]
    fn failed_and_unavailable_answers_are_distinguished() {
        let failed = Overview::build(
            &ControlReply::failed("dispatch error"),
            &ControlReply::unavailable("not composed"),
        );
        assert_eq!(failed.router_info_availability, Availability::Failed);
        assert_eq!(failed.services_availability, Availability::Unavailable);

        let unsupported = Overview::build(
            &ControlReply {
                availability: Availability::Unsupported,
                value: Value::Object(serde_json::Map::new()),
                detail: Some("not implemented".to_string()),
            },
            &ControlReply::unavailable("not composed"),
        );
        for metric in &unsupported.metrics {
            assert_eq!(metric.availability, Availability::Unsupported);
            assert!(metric.value.is_none());
        }
    }

    #[test]
    fn service_rows_are_bounded_and_ordered_by_the_control_object() {
        let mut map = serde_json::Map::new();
        for index in 0..(MAX_SERVICE_ROWS + 20) {
            map.insert(format!("service{index}"), json!("running"));
        }
        let overview = Overview::build(
            &ControlReply::unavailable("none"),
            &reply(Value::Object(map)),
        );
        assert_eq!(overview.services.len(), MAX_SERVICE_ROWS);
    }

    #[test]
    fn object_valued_fields_are_never_summarised_into_a_scalar() {
        // `field` is the scalar accessor: an object is never flattened
        // into a single invented value.
        let control = reply(json!({ "i2p.router.status": { "nested": true } }));
        assert_eq!(control.field("i2p.router.status"), None);
    }

    #[test]
    fn service_objects_render_bounded_scalar_leaves() {
        let overview = Overview::build(
            &ControlReply::unavailable("none"),
            &reply(json!({ "SAM": { "enabled": false, "port": 7656 } })),
        );
        assert_eq!(overview.services.len(), 1);
        let row = &overview.services[0];
        assert_eq!(row.name, "SAM");
        assert_eq!(row.state.as_deref(), Some("enabled=false, port=7656"));
        assert_eq!(row.availability, Availability::Returned);
    }

    #[test]
    fn a_service_with_no_scalar_leaf_is_unavailable_not_empty() {
        let overview = Overview::build(
            &ControlReply::unavailable("none"),
            &reply(json!({ "BOB": { "nested": { "deeper": true } } })),
        );
        assert_eq!(overview.services.len(), 1);
        assert_eq!(overview.services[0].state, None);
        assert_ne!(overview.services[0].availability, Availability::Returned);
    }

    #[test]
    fn presented_values_are_length_bounded() {
        let long = "x".repeat(MAX_VALUE_LEN * 2);
        let control = reply(json!({ "i2p.router.version": long }));
        let value = control.field("i2p.router.version").expect("value present");
        assert_eq!(value.chars().count(), MAX_VALUE_LEN);
    }

    #[test]
    fn json_output_reports_availability_per_row() {
        let overview = Overview::build(
            &reply(json!({ "i2p.router.version": "0.1.0" })),
            &ControlReply::unavailable("none"),
        );
        let document = overview.to_json();
        assert_eq!(document["routerInfo"].as_str(), Some("returned"));
        assert_eq!(document["clientServices"].as_str(), Some("unavailable"));
        let first = &document["metrics"][0];
        assert_eq!(first["label"].as_str(), Some("Version"));
        assert_eq!(first["value"].as_str(), Some("0.1.0"));
        assert_eq!(first["availability"].as_str(), Some("returned"));
        let second = &document["metrics"][1];
        assert!(second["value"].is_null());
        assert_ne!(second["availability"].as_str(), Some("returned"));
    }

    #[test]
    fn the_unavailable_client_reports_nothing_rather_than_failing() {
        let overview = Overview::build(
            &UnavailableControlClient.router_info(),
            &UnavailableControlClient.client_services(),
        );
        assert!(overview.is_empty());
        assert_eq!(overview.router_info_availability, Availability::Unavailable);
        // Serialization still succeeds, which is what keeps the API
        // contract stable when sources are absent.
        assert!(overview.to_json()["metrics"].is_array());
    }

    #[test]
    fn overview_row_counts_stay_inside_the_ceilings() {
        assert!(METRICS.len() <= MAX_OVERVIEW_ROWS);
    }
}
