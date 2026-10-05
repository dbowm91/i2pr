//! Listener-independent Proposal-170 control dispatch (Plan 358).
//!
//! The read-only methods `RouterInfo` and `ClientServicesInfo` never touch
//! the TCP/TLS listener, the service password, or the bearer-token table.
//! Hoisting them into this owner lets the built-in console reach the *same*
//! canonical dispatch without binding an external listener, requiring an
//! I2PControl password, or minting a token.
//!
//! There is exactly one implementation of each method. The external
//! listener and the local console principal both call the functions here,
//! so a divergence between them is not possible by construction rather
//! than by review.
//!
//! What the local principal is *not*: it is not a bearer token, it is not
//! accepted from a request, and it cannot name a method. The allow-set in
//! `LocalConsolePrincipal` is closed and read-only.

use std::sync::Arc;
use std::time::Duration;

use i2pr_i2pcontrol::{
    JsonRpcErrorCode, RequestId as JsonRpcRequestId, error_envelope, success_envelope,
};

use super::i2pcontrol::proposal_router_info_value_matches;
use super::i2pcontrol_inspection::{
    InspectionHandles, client_service_result, router_info_result, select_client_services,
    select_router_info,
};

/// The canonical owner of Proposal-170 method dispatch.
///
/// Construction requires only the inspection handles, so the console can
/// build one whether or not the external I2PControl listener is enabled.
#[derive(Clone, Debug)]
pub(crate) struct ControlDispatcher {
    inspection: Arc<InspectionHandles>,
}

impl ControlDispatcher {
    /// Builds a dispatcher over the shared inspection handles.
    pub(crate) fn new(inspection: Arc<InspectionHandles>) -> Self {
        Self { inspection }
    }

    /// Dispatches an authenticated `RouterInfo` request over the base API
    /// and Proposal 170 selector namespaces.
    ///
    /// Selector values are ignored; unknown keys fail with invalid params.
    /// An empty
    /// selection answers with an empty result object. Any unavailable or
    /// unpublished selection fails the whole request explicitly with the
    /// owning-plan marker; no partial response is emitted and no state is
    /// mutated.
    pub(crate) fn process_router_info(
        &self,
        id: Option<&JsonRpcRequestId>,
        params: &serde_json::Map<String, serde_json::Value>,
        now_ms: u64,
    ) -> (serde_json::Value, Duration) {
        let selection = match select_router_info(params) {
            Ok(selection) => selection,
            Err(_) => {
                return (
                    error_envelope(
                        id,
                        JsonRpcErrorCode::InvalidParams.code(),
                        JsonRpcErrorCode::InvalidParams.message(),
                    ),
                    Duration::ZERO,
                );
            }
        };
        // Control-plane uptime in whole seconds (truncating, saturating).
        let uptime_secs = now_ms / 1000;
        let mut result = serde_json::Map::with_capacity(selection.len());
        let mut clear_logs = false;
        for field in selection {
            if field.key == "i2p.router.net.tunnels.i2ptunnel" {
                match crate::i2pcontrol_inspection::proposal_i2ptunnel_summaries(&self.inspection) {
                    Ok(value) => {
                        result.insert(field.key.to_owned(), value);
                        continue;
                    }
                    Err(gap) => {
                        return (
                            error_envelope(
                                id,
                                JsonRpcErrorCode::InternalError.code(),
                                &gap.message(),
                            ),
                            Duration::ZERO,
                        );
                    }
                }
            }
            if matches!(
                field.key,
                "i2p.router.net.total.transit.bytes"
                    | "i2p.router.net.bw.transit.15s"
                    | "i2p.router.net.tunnels.shareratio"
            ) {
                // Plan 340: the value is the transit participation posture
                // and the volume its forward path measured. Any gap fails
                // the whole request closed rather than returning a partial
                // RouterInfo result.
                match crate::i2pcontrol_inspection::proposal_transit_volume(
                    field.key,
                    &self.inspection,
                ) {
                    Ok(value) => {
                        result.insert(field.key.to_owned(), value);
                        continue;
                    }
                    Err(gap) => {
                        return (
                            error_envelope(
                                id,
                                JsonRpcErrorCode::InternalError.code(),
                                &gap.message(),
                            ),
                            Duration::ZERO,
                        );
                    }
                }
            }
            if matches!(
                field.key,
                "i2p.router.net.total.received.bytes" | "i2p.router.net.total.sent.bytes"
            ) {
                match crate::i2pcontrol_inspection::proposal_transport_total(
                    field.key,
                    &self.inspection,
                ) {
                    Ok(value) => {
                        result.insert(field.key.to_owned(), value);
                        continue;
                    }
                    Err(gap) => {
                        return (
                            error_envelope(
                                id,
                                JsonRpcErrorCode::InternalError.code(),
                                &gap.message(),
                            ),
                            Duration::ZERO,
                        );
                    }
                }
            }
            if matches!(
                field.key,
                "i2p.router.netdb.ntcp.limit" | "i2p.router.netdb.ssu.limit"
            ) {
                match self.inspection.proposal_connection_limit(field.key) {
                    Some(limit) => {
                        result.insert(field.key.to_owned(), serde_json::Value::from(limit));
                        continue;
                    }
                    None => {
                        return (
                            error_envelope(
                                id,
                                JsonRpcErrorCode::InternalError.code(),
                                "RouterInfo transport connection limit is unavailable",
                            ),
                            Duration::ZERO,
                        );
                    }
                }
            }
            if matches!(
                field.key,
                "i2p.router.net.status.v6"
                    | "i2p.router.net.error"
                    | "i2p.router.net.error.v6"
                    | "i2p.router.net.testing"
                    | "i2p.router.net.testing.v6"
            ) {
                match crate::i2pcontrol_inspection::proposal_network_condition_value(
                    field.key,
                    &self.inspection,
                ) {
                    Ok(value) => {
                        result.insert(field.key.to_owned(), value);
                        continue;
                    }
                    Err(gap) => {
                        return (
                            error_envelope(
                                id,
                                JsonRpcErrorCode::InternalError.code(),
                                &gap.message(),
                            ),
                            Duration::ZERO,
                        );
                    }
                }
            }
            if field.key == "i2p.router.net.tunnels.tbmqueue" {
                match crate::i2pcontrol_inspection::proposal_tbm_queue_depth(&self.inspection) {
                    Ok(value) => {
                        result.insert(field.key.to_owned(), value);
                        continue;
                    }
                    Err(gap) => {
                        return (
                            error_envelope(
                                id,
                                JsonRpcErrorCode::InternalError.code(),
                                &gap.message(),
                            ),
                            Duration::ZERO,
                        );
                    }
                }
            }
            if matches!(
                field.key,
                "i2p.router.netdb.activepeers.info"
                    | "i2p.router.netdb.activepeers.stats"
                    | "i2p.router.netdb.peers.info"
            ) {
                match crate::i2pcontrol_inspection::proposal_empty_router_info_list(
                    field.key,
                    &self.inspection,
                ) {
                    Ok(value) => {
                        result.insert(field.key.to_owned(), value);
                        continue;
                    }
                    Err(gap) => {
                        return (
                            error_envelope(
                                id,
                                JsonRpcErrorCode::InternalError.code(),
                                &gap.message(),
                            ),
                            Duration::ZERO,
                        );
                    }
                }
            }
            if field.key == "i2p.router.netdb.bannedpeers" {
                match crate::i2pcontrol_inspection::proposal_empty_banned_peer_details(
                    &self.inspection,
                ) {
                    Ok(value) => {
                        result.insert(field.key.to_owned(), value);
                        continue;
                    }
                    Err(gap) => {
                        return (
                            error_envelope(
                                id,
                                JsonRpcErrorCode::InternalError.code(),
                                &gap.message(),
                            ),
                            Duration::ZERO,
                        );
                    }
                }
            }
            match field.key {
                "i2p.router.news" => {
                    let news = match self.inspection.proposal_news(now_ms / 1000) {
                        Some(news) => news,
                        None => {
                            return (
                                error_envelope(
                                    id,
                                    JsonRpcErrorCode::InternalError.code(),
                                    "Router news unavailable: no verified NEWS feed has been published (Plan 322)",
                                ),
                                Duration::ZERO,
                            );
                        }
                    };
                    result.insert(
                        field.key.to_owned(),
                        serde_json::Value::String(news.rendered),
                    );
                    continue;
                }
                "i2p.router.clockskew" => {
                    // No peer-skew sample is collected yet; the Proposal
                    // explicitly permits null when there are no observations.
                    result.insert(field.key.to_owned(), serde_json::Value::Null);
                    continue;
                }
                "i2p.router.info" => {
                    result.insert(
                        field.key.to_owned(),
                        crate::i2pcontrol_inspection::proposal_local_router_info(&self.inspection),
                    );
                    continue;
                }
                "i2p.router.id" => {
                    let identity = router_info_result(
                        i2pr_i2pcontrol::RouterInfoSelector::RouterHash,
                        &self.inspection,
                        uptime_secs,
                    )
                    .unwrap_or(serde_json::Value::Null);
                    result.insert(field.key.to_owned(), identity);
                    continue;
                }
                _ => {}
            }
            if field.key.starts_with("i2p.router.addressbook.") {
                match crate::i2pcontrol_inspection::proposal_addressbook_value(
                    field.key,
                    &self.inspection,
                ) {
                    Ok(value) => {
                        result.insert(field.key.to_owned(), value);
                        continue;
                    }
                    Err(gap) => {
                        return (
                            error_envelope(
                                id,
                                JsonRpcErrorCode::InternalError.code(),
                                &gap.message(),
                            ),
                            Duration::ZERO,
                        );
                    }
                }
            }
            if field.key == "i2p.router.logs" {
                let Some(lines) = self.inspection.recent_logs() else {
                    return (
                        error_envelope(
                            id,
                            JsonRpcErrorCode::InternalError.code(),
                            "RouterInfo selector source is unavailable",
                        ),
                        Duration::ZERO,
                    );
                };
                result.insert(field.key.to_owned(), serde_json::json!(lines));
                continue;
            }
            if field.key == "i2p.router.logs.clear" {
                clear_logs = true;
                continue;
            }
            if field.key == "i2p.router.uptime" {
                result.insert(field.key.to_owned(), serde_json::Value::from(now_ms));
                continue;
            }
            if matches!(
                field.key,
                "i2p.router.net.tunnels.successrate" | "i2p.router.net.tunnels.totalsuccessrate"
            ) {
                match crate::i2pcontrol_inspection::proposal_tunnel_success_rate(
                    field.key,
                    &self.inspection,
                ) {
                    Ok(value) => {
                        result.insert(field.key.to_owned(), value);
                        continue;
                    }
                    Err(gap) => {
                        return (
                            error_envelope(
                                id,
                                JsonRpcErrorCode::InternalError.code(),
                                &gap.message(),
                            ),
                            Duration::ZERO,
                        );
                    }
                }
            }
            if field.key == "i2p.router.net.tunnels.queue" {
                match crate::i2pcontrol_inspection::proposal_tunnel_queue_depth(&self.inspection) {
                    Ok(value) => {
                        result.insert(field.key.to_owned(), value);
                        continue;
                    }
                    Err(gap) => {
                        return (
                            error_envelope(
                                id,
                                JsonRpcErrorCode::InternalError.code(),
                                &gap.message(),
                            ),
                            Duration::ZERO,
                        );
                    }
                }
            }
            if matches!(
                field.key,
                "i2p.router.net.tunnels.exploratory.inbound"
                    | "i2p.router.net.tunnels.exploratory.outbound"
                    | "i2p.router.net.tunnels.exploratory.info.list"
                    | "i2p.router.net.tunnels.client.inbound"
                    | "i2p.router.net.tunnels.client.outbound"
                    | "i2p.router.net.tunnels.client.info.list"
                    | "i2p.router.net.tunnels.participating.info"
            ) {
                match crate::i2pcontrol_inspection::proposal_empty_tunnel_projection(
                    field.key,
                    &self.inspection,
                ) {
                    Ok(value) => {
                        result.insert(field.key.to_owned(), value);
                        continue;
                    }
                    Err(gap) => {
                        return (
                            error_envelope(
                                id,
                                JsonRpcErrorCode::InternalError.code(),
                                &gap.message(),
                            ),
                            Duration::ZERO,
                        );
                    }
                }
            }
            let Some(selector) = field.adapter else {
                let message = crate::i2pcontrol_inspection::proposal_unavailable_gap(field.key)
                    .map(|gap| gap.message())
                    .unwrap_or_else(|| "RouterInfo selector source is unavailable".to_owned());
                return (
                    error_envelope(id, JsonRpcErrorCode::InternalError.code(), &message),
                    Duration::ZERO,
                );
            };
            match router_info_result(selector, &self.inspection, uptime_secs) {
                Ok(value) => {
                    let value = match field.key {
                        "i2p.router.netdb.knownpeers" | "i2p.router.netdb.activepeers" => {
                            match value.as_array() {
                                Some(peers) => serde_json::Value::from(peers.len() as u64),
                                None => {
                                    return (
                                        error_envelope(
                                            id,
                                            JsonRpcErrorCode::InternalError.code(),
                                            "RouterInfo owner returned an invalid field shape",
                                        ),
                                        Duration::ZERO,
                                    );
                                }
                            }
                        }
                        _ => value,
                    };
                    result.insert(field.key.to_owned(), value);
                }
                Err(gap) => {
                    return (
                        error_envelope(id, JsonRpcErrorCode::InternalError.code(), &gap.message()),
                        Duration::ZERO,
                    );
                }
            }
        }
        let shapes_valid = result
            .iter()
            .all(|(key, value)| proposal_router_info_value_matches(key, value))
            && (!clear_logs
                || proposal_router_info_value_matches(
                    "i2p.router.logs.clear",
                    &serde_json::Value::String("success".to_owned()),
                ));
        if !shapes_valid {
            return (
                error_envelope(
                    id,
                    JsonRpcErrorCode::InternalError.code(),
                    "RouterInfo owner returned a value outside the canonical Proposal type",
                ),
                Duration::ZERO,
            );
        }
        // Defer the side effect until every requested value has resolved,
        // so a mixed selection cannot partially mutate on an error.
        if clear_logs {
            if self.inspection.clear_logs().is_none() {
                return (
                    error_envelope(
                        id,
                        JsonRpcErrorCode::InternalError.code(),
                        "RouterInfo selector source is unavailable",
                    ),
                    Duration::ZERO,
                );
            }
            result.insert(
                "i2p.router.logs.clear".to_owned(),
                serde_json::Value::String("success".to_owned()),
            );
        }
        (
            success_envelope(id, serde_json::Value::Object(result)),
            Duration::ZERO,
        )
    }
    /// Dispatches an authenticated `ClientServicesInfo` request over the
    /// same select form. Every service row answers (disabled is truthful
    /// state), so this path is infallible after select validation.
    pub(crate) fn process_client_services(
        &self,
        id: Option<&JsonRpcRequestId>,
        params: &serde_json::Map<String, serde_json::Value>,
    ) -> (serde_json::Value, Duration) {
        let selection = match select_client_services(params) {
            Ok(selection) => selection,
            Err(_) => {
                return (
                    error_envelope(
                        id,
                        JsonRpcErrorCode::InvalidParams.code(),
                        JsonRpcErrorCode::InvalidParams.message(),
                    ),
                    Duration::ZERO,
                );
            }
        };
        let mut result = serde_json::Map::with_capacity(selection.len());
        for service in selection {
            result.insert(
                service.name().to_owned(),
                client_service_result(service, &self.inspection),
            );
        }
        (
            success_envelope(id, serde_json::Value::Object(result)),
            Duration::ZERO,
        )
    }
}

/// The capability the built-in console holds.
///
/// It bypasses **only** the external bearer-token transport step. It does
/// not bypass method validation, parameter validation, selector
/// validation, source availability rules, result bounds, or redaction:
/// those all still run in the functions above.
///
/// The unit struct cannot be named or constructed outside this module, so
/// no browser-supplied value can ever become one.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct LocalConsolePrincipal {
    _private: (),
}

impl LocalConsolePrincipal {
    /// Issues the principal. Only the daemon composition root calls this.
    pub(crate) fn issue() -> Self {
        Self { _private: () }
    }

    /// Returns whether a method is inside the local read-only allow-set.
    pub(crate) fn permits(method: &i2pr_i2pcontrol::Method) -> bool {
        matches!(
            method,
            i2pr_i2pcontrol::Method::RouterInfo | i2pr_i2pcontrol::Method::ClientServicesInfo
        )
    }

    /// Dispatches one read-only method for the local console.
    ///
    /// Anything outside the allow-set is refused with the canonical
    /// JSON-RPC error vocabulary rather than silently ignored, so a future
    /// caller that tries to widen the set gets a test failure first.
    pub(crate) fn dispatch_readonly(
        &self,
        dispatcher: &ControlDispatcher,
        method: &i2pr_i2pcontrol::Method,
        id: Option<&JsonRpcRequestId>,
        params: &serde_json::Map<String, serde_json::Value>,
        now_ms: u64,
    ) -> (serde_json::Value, Duration) {
        if !Self::permits(method) {
            return (
                error_envelope(
                    id,
                    JsonRpcErrorCode::MethodNotFound.code(),
                    JsonRpcErrorCode::MethodNotFound.message(),
                ),
                Duration::ZERO,
            );
        }
        match method {
            i2pr_i2pcontrol::Method::RouterInfo => {
                dispatcher.process_router_info(id, params, now_ms)
            }
            i2pr_i2pcontrol::Method::ClientServicesInfo => {
                dispatcher.process_client_services(id, params)
            }
            // Unreachable: `permits` admits exactly the two arms above.
            _ => (
                error_envelope(
                    id,
                    JsonRpcErrorCode::MethodNotFound.code(),
                    JsonRpcErrorCode::MethodNotFound.message(),
                ),
                Duration::ZERO,
            ),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::{IpAddr, Ipv4Addr};

    use crate::config::{Config, I2pControlConfig};
    use crate::i2pcontrol::{HttpDispatch, I2pControlServiceState};

    fn config() -> Config {
        Config::parse(concat!(
            "schema_version = 1\n",
            "[router]\n",
            "data_dir = \"./state\"\n",
            "[network]\n",
            "bind_address = \"127.0.0.1\"\n",
            "listen_port = 9150\n",
            "network_id = 2\n",
        ))
        .expect("fixture config parses")
    }

    fn inspection() -> Arc<InspectionHandles> {
        Arc::new(InspectionHandles::from_config(&config()))
    }

    fn control_config() -> I2pControlConfig {
        Config::parse(concat!(
            "schema_version = 1\n",
            "[router]\n",
            "data_dir = \"state\"\n",
            "[i2pcontrol]\n",
            "enabled = true\n",
            "bind_address = \"127.0.0.1\"\n",
            "port = 0\n",
            "password = \"parity-password\"\n",
        ))
        .expect("control config parses")
        .i2pcontrol
    }

    /// Authenticates over the real external entry point and returns the
    /// bearer token.
    fn external_authenticate(service: &I2pControlServiceState) -> String {
        let body = concat!(
            "{\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"Authenticate\",",
            "\"params\":{\"API\":1,\"Password\":\"parity-password\"}}"
        );
        let outcome = futures_executor::block_on(service.dispatch_body(
            body.as_bytes(),
            None,
            IpAddr::V4(Ipv4Addr::LOCALHOST),
            0,
        ));
        let HttpDispatch::Json(bytes) = outcome.dispatch else {
            panic!("authenticate must answer a JSON body");
        };
        let parsed: serde_json::Value =
            serde_json::from_slice(&bytes).expect("response is valid JSON");
        parsed
            .get("result")
            .and_then(|result| result.get("Token"))
            .and_then(|token| token.as_str())
            .expect("authenticate mints a token")
            .to_string()
    }

    fn external_router_info(service: &I2pControlServiceState, body: &str) -> serde_json::Value {
        let outcome = futures_executor::block_on(service.dispatch_body(
            body.as_bytes(),
            None,
            IpAddr::V4(Ipv4Addr::LOCALHOST),
            0,
        ));
        let HttpDispatch::Json(bytes) = outcome.dispatch else {
            panic!("external dispatch must answer a JSON body");
        };
        let parsed: serde_json::Value =
            serde_json::from_slice(&bytes).expect("response is valid JSON");
        parsed
            .get("result")
            .cloned()
            .unwrap_or_else(|| panic!("external response carries a result, got: {parsed}"))
    }

    #[test]
    fn local_principal_admits_only_the_read_only_methods() {
        assert!(LocalConsolePrincipal::permits(
            &i2pr_i2pcontrol::Method::RouterInfo
        ));
        assert!(LocalConsolePrincipal::permits(
            &i2pr_i2pcontrol::Method::ClientServicesInfo
        ));
        for denied in [
            i2pr_i2pcontrol::Method::Authenticate,
            i2pr_i2pcontrol::Method::AddressBook,
            i2pr_i2pcontrol::Method::TunnelManager,
        ] {
            assert!(
                !LocalConsolePrincipal::permits(&denied),
                "{denied:?} must stay outside the local allow-set"
            );
        }
    }

    #[test]
    fn every_overview_selector_is_a_canonical_contract_field() {
        // The console lists its selectors as data, so the contract must be
        // checked against them somewhere. This is the place where both are
        // visible.
        for selector in i2pr_console::control::OVERVIEW_SELECTORS {
            assert!(
                i2pr_i2pcontrol::router_info_field(selector).is_some(),
                "{selector} is not a canonical RouterInfo field"
            );
        }
    }

    #[test]
    fn verified_base_overview_selectors_answer() {
        // The verified base set is a claim about this implementation, so it
        // is proved here rather than assumed.
        let principal = LocalConsolePrincipal::issue();
        let dispatcher = ControlDispatcher::new(inspection());
        for selector in crate::console::requestable_overview_selectors() {
            let mut params = serde_json::Map::new();
            params.insert((*selector).to_string(), serde_json::Value::Null);
            let (envelope, _) = principal.dispatch_readonly(
                &dispatcher,
                &i2pr_i2pcontrol::Method::RouterInfo,
                None,
                &params,
                0,
            );
            assert!(
                envelope.get("result").is_some(),
                "{selector} is claimed publishable but answered: {envelope}"
            );
        }
    }

    #[test]
    fn the_overview_requests_a_strict_subset_so_publish_gated_rows_are_withheld() {
        // Requesting a publish-gated or unavailable row fails the entire
        // canonical request, so the console must withhold some selectors and
        // present them as unavailable instead.
        let selection = crate::console::requestable_overview_selectors();
        assert!(
            !selection.is_empty(),
            "at least one overview selector must be requestable"
        );
        for selector in &selection {
            assert!(
                i2pr_console::control::OVERVIEW_SELECTORS.contains(selector),
                "{selector} is not part of the overview selector set"
            );
        }
        assert!(
            selection.len() < i2pr_console::control::OVERVIEW_SELECTORS.len(),
            "the withholding path must actually be exercised: {selection:?}"
        );
    }

    #[test]
    fn local_principal_result_matches_the_external_wire_result() {
        let principal = LocalConsolePrincipal::issue();
        let dispatcher = ControlDispatcher::new(inspection());
        let selection = crate::console::requestable_overview_selectors();
        assert!(
            !selection.is_empty(),
            "the canonical matrix must yield at least one requestable overview selector"
        );
        let mut params = serde_json::Map::new();
        for selector in &selection {
            params.insert((*selector).to_string(), serde_json::Value::Null);
        }
        let (envelope, _) = principal.dispatch_readonly(
            &dispatcher,
            &i2pr_i2pcontrol::Method::RouterInfo,
            None,
            &params,
            0,
        );
        let local = envelope
            .get("result")
            .cloned()
            .unwrap_or_else(|| panic!("local principal must reach a result, got: {envelope}"));

        let mut body_params = String::from("\"Token\":\"\"");

        // The external path must mint a token first: an unauthenticated
        // read is refused, which is the whole difference between the two
        // surfaces.
        let service = I2pControlServiceState::new_with_inspection(control_config(), inspection())
            .expect("control service constructs");
        let unauthenticated = futures_executor::block_on(service.dispatch_body(
            format!("{{\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"RouterInfo\",\"params\":{{{body_params}}}}}").as_bytes(),
            None,
            IpAddr::V4(Ipv4Addr::LOCALHOST),
            0,
        ));
        let HttpDispatch::Json(refused) = unauthenticated.dispatch else {
            panic!("unauthenticated request must still answer JSON");
        };
        let refused: serde_json::Value =
            serde_json::from_slice(&refused).expect("response is valid JSON");
        assert!(
            refused.get("error").is_some(),
            "the external surface must refuse an unauthenticated read"
        );

        let token = external_authenticate(&service);
        body_params = format!("\"Token\":\"{token}\"");
        for selector in &selection {
            body_params.push_str(&format!(",\"{selector}\":null"));
        }
        let authenticated_body = format!(
            "{{\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"RouterInfo\",\"params\":{{{body_params}}}}}"
        );
        let external = external_router_info(&service, &authenticated_body);
        assert_eq!(
            local, external,
            "the local principal must not become a richer private API"
        );
    }

    #[test]
    fn an_unpublished_selector_fails_honestly_rather_than_reporting_zero() {
        // `i2p.router.net.tunnels.successrate` is a canonical field whose
        // source this router does not publish (Plan 322). Asking for it
        // must produce a refusal, not a fabricated zero.
        let principal = LocalConsolePrincipal::issue();
        let dispatcher = ControlDispatcher::new(inspection());
        let mut params = serde_json::Map::new();
        params.insert(
            "i2p.router.net.tunnels.successrate".to_string(),
            serde_json::Value::Null,
        );
        let (envelope, _) = principal.dispatch_readonly(
            &dispatcher,
            &i2pr_i2pcontrol::Method::RouterInfo,
            None,
            &params,
            0,
        );
        assert!(
            envelope.get("error").is_some(),
            "an unpublished selector must be refused, got: {envelope}"
        );
        assert!(
            envelope.get("result").is_none(),
            "a refused selector must not also return a value"
        );

        // And the console classifies it as unavailable, not as zero.
        let reply = crate::console::classify_envelope(&envelope);
        assert_eq!(reply.availability, i2pr_console::Availability::Failed);
        assert_eq!(reply.field("i2p.router.net.tunnels.successrate"), None);
    }

    #[test]
    fn console_only_configuration_needs_no_control_password_or_listener() {
        let console_only = Config::parse(concat!(
            "schema_version = 1\n",
            "[router]\n",
            "data_dir = \"./state\"\n",
            "[console]\n",
            "enabled = true\n",
        ))
        .expect("console-only config parses");
        assert!(console_only.console.enabled);
        assert!(
            !console_only.i2pcontrol.enabled,
            "enabling the console must not enable the external listener"
        );
        assert!(console_only.console.password_hash.is_none());
        assert!(console_only.console.bind_address.is_loopback());
    }

    #[test]
    fn console_does_not_enable_the_external_listener_by_its_own() {
        let with_console = Config::parse(concat!(
            "schema_version = 1\n",
            "[router]\n",
            "data_dir = \"./state\"\n",
            "[console]\n",
            "enabled = true\n",
        ))
        .expect("config parses");
        assert!(with_console.console.enabled);
        assert!(!with_console.i2pcontrol.enabled);
        assert!(!with_console.sam.enabled);
        assert!(!with_console.i2cp.enabled);
    }
}
