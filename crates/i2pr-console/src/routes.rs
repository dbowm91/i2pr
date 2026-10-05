//! Route handlers, the guard stack, and the server-rendered shell.
//!
//! Every response, including every rejection, is produced through
//! `respond` so the centralized security-header policy cannot be
//! bypassed by adding a route later. The guard stack runs as middleware in
//! the fixed order defined by [`crate::security`], before any handler.

use std::sync::Arc;

use axum::Router;
use axum::body::Body;
use axum::extract::{Extension, Form, Path, State};
use axum::http::{HeaderName, HeaderValue, Request, Response, StatusCode, header};
use axum::middleware::{self, Next};
use axum::routing::{get, post};
use serde::Deserialize;

use crate::assets;
use crate::control::Overview;
use crate::html::Text;
use crate::security::authority::RequestSafety;
use crate::security::headers::{
    CSRF_COOKIE_NAME, CachePolicy, SESSION_COOKIE_NAME, apply_policy, clear_csrf_cookie_header,
    clear_session_cookie_header, csrf_cookie_header, has_cors_header, session_cookie_header,
};
use crate::security::{RequestGuard, SecurityPolicy};
use crate::theme::ThemePalette;
use crate::{AppRouter, ConsoleSecret, ConsoleState, now_secs};

/// The request header map, aliased for readability.
type HeaderMap = axum::http::HeaderMap;

/// Assembles the hardened console router.
pub fn build(state: Arc<ConsoleState>) -> AppRouter {
    // The guard stack reads the shared state from a request extension
    // rather than the router's typed state, because axum's `from_fn`
    // middleware cannot take a `State` extractor.
    let for_guard = Arc::clone(&state);
    // Axum applies the *last* layer first, so the extension layer is added
    // last and therefore runs before the guard observes the request.
    bare(state)
        .layer(middleware::from_fn(guard_stack))
        .layer(Extension(for_guard))
}

/// Assembles the router without the guard stack.
pub fn bare(state: Arc<ConsoleState>) -> AppRouter {
    Router::new()
        .route("/", get(index))
        .route("/login", get(login_form).post(login_submit))
        .route("/logout", post(logout))
        .route("/assets/{name}", get(asset))
        .route("/theme.css", get(theme_css))
        .route("/api/overview", get(overview_json))
        .route("/api/fixture/mark", post(fixture_mark))
        .fallback(not_found)
        .with_state(state)
}

// ---------------------------------------------------------------------------
// Response construction
// ---------------------------------------------------------------------------

/// Builds the one sanctioned console response.
///
/// Status and headers are assigned directly rather than through a fallible
/// builder, so no handler can panic on its own output, and the security
/// policy is applied on the single path every response takes.
fn respond(status: StatusCode, content_type: &'static str, body: String) -> Response<Body> {
    let mut response = Response::new(Body::from(body));
    *response.status_mut() = status;
    apply_policy(&mut response, content_type, CachePolicy::NoStore);
    response
}

/// Builds a compiled-asset response, which may use the narrower cache rule.
fn asset_response(status: StatusCode, content_type: &'static str, body: &str) -> Response<Body> {
    let mut response = Response::new(Body::from(body.to_string()));
    *response.status_mut() = status;
    apply_policy(&mut response, content_type, CachePolicy::ImmutableAsset);
    response
}

/// Appends a `Set-Cookie` value to a response.
fn with_cookie(response: &mut Response<Body>, value: String) {
    if let Ok(header_value) = HeaderValue::from_str(&value) {
        response
            .headers_mut()
            .append(header::SET_COOKIE, header_value);
    }
}

// ---------------------------------------------------------------------------
// Guard stack
// ---------------------------------------------------------------------------

/// Runs the ordered browser guard stack.
///
/// Order (Plan 357): authority validation, then cross-origin policy for
/// unsafe methods, then session authentication, then CSRF. A refusal is
/// produced here and never reaches a handler, so no handler can forget a
/// check.
async fn guard_stack(request: Request<Body>, next: Next) -> Response<Body> {
    let Some(state) = request.extensions().get::<Arc<ConsoleState>>().cloned() else {
        // The router always installs the extension; without it there is no
        // policy to enforce, so the request is refused rather than served
        // unguarded.
        return respond(
            StatusCode::INTERNAL_SERVER_ERROR,
            "text/plain; charset=utf-8",
            "console unavailable\n".to_string(),
        );
    };
    let policy = state.security();
    let method = request.method().as_str().to_string();
    let path = request.uri().path().to_string();

    let headers = request.headers();
    let host = headers.get(header::HOST).and_then(|v| v.to_str().ok());
    let origin = headers.get(header::ORIGIN).and_then(|v| v.to_str().ok());
    let referer = headers.get(header::REFERER).and_then(|v| v.to_str().ok());
    let cookies = read_cookies(headers.get(header::COOKIE).and_then(|v| v.to_str().ok()));

    let guard = RequestGuard::new(
        &method,
        host,
        origin,
        referer,
        cookies.get(SESSION_COOKIE_NAME).map(String::as_str),
        cookies.get(CSRF_COOKIE_NAME).map(String::as_str),
    );
    let requirement = SecurityPolicy::requirement_for(policy, &path);

    if !guard.admit(policy, now_secs(), requirement, &path) {
        return refusal(policy, &method);
    }
    next.run(request).await
}

/// Returns the refusal response for a rejected request.
///
/// An authority or origin failure is a `403`; the distinction is
/// deliberately not surfaced in the body, which is a fixed string.
fn refusal(policy: &SecurityPolicy, method: &str) -> Response<Body> {
    let _ = (policy, method);
    respond(
        StatusCode::FORBIDDEN,
        "text/plain; charset=utf-8",
        "forbidden\n".to_string(),
    )
}

/// Parses a `Cookie` header into name/value pairs.
///
/// Parsing is bounded: only the two console cookies are retained, each is
/// length-capped, and a duplicate name keeps the first occurrence so a
/// second injected copy cannot override an earlier legitimate value.
fn read_cookies(raw: Option<&str>) -> std::collections::HashMap<String, String> {
    let mut cookies = std::collections::HashMap::new();
    let Some(raw) = raw else {
        return cookies;
    };
    if raw.len() > 4096 {
        return cookies;
    }
    for pair in raw.split(';') {
        let Some((name, value)) = pair.split_once('=') else {
            continue;
        };
        let name = name.trim();
        if name != SESSION_COOKIE_NAME && name != CSRF_COOKIE_NAME {
            continue;
        }
        let value = value.trim();
        if value.is_empty() || value.len() > crate::security::session::MAX_COOKIE_VALUE_LEN {
            continue;
        }
        cookies
            .entry(name.to_string())
            .or_insert_with(|| value.to_string());
    }
    cookies
}

// ---------------------------------------------------------------------------
// Handlers
// ---------------------------------------------------------------------------

/// Builds a redirect through the centralized response policy.
///
/// A bare `axum::response::Redirect` would bypass
/// [`respond`] and ship without the hardening headers. Every redirect in
/// this module is constructed here so that cannot happen.
fn redirect(status: StatusCode, location: &'static str) -> Response<Body> {
    let mut response = respond(
        status,
        "text/plain; charset=utf-8",
        "redirect\n".to_string(),
    );
    response
        .headers_mut()
        .insert(header::LOCATION, HeaderValue::from_static(location));
    response
}

/// Serves the console shell, or redirects to the login page.
///
/// The shell is a public path so the login page is always styled and
/// reachable; the authenticated-mode redirect is therefore a handler
/// decision, made once, rather than a route-table special case.
pub async fn index(State(state): State<Arc<ConsoleState>>, headers: HeaderMap) -> Response<Body> {
    let policy = state.security();
    if policy.mode().requires_sessions() && extract_session(&state, &headers).is_none() {
        return redirect(StatusCode::TEMPORARY_REDIRECT, "/login");
    }
    let csrf = session_csrf(&state, &headers);
    let overview = build_overview(&state);
    respond(
        StatusCode::OK,
        "text/html; charset=utf-8",
        render_shell(
            state.config().theme(),
            state.config().title(),
            csrf.as_deref(),
            &overview,
        ),
    )
}

/// Serves the generated theme stylesheet.
pub async fn theme_css(State(state): State<Arc<ConsoleState>>) -> Response<Body> {
    asset_response(
        StatusCode::OK,
        "text/css; charset=utf-8",
        &state.config().theme().css_variables(),
    )
}

/// Serves one compile-time asset, or a fixed 404.
pub async fn asset(Path(name): Path<String>) -> Response<Body> {
    match assets::find(&name).copied() {
        Some(asset) => asset_response(StatusCode::OK, asset.content_type, asset.body),
        None => not_found_response(),
    }
}

/// The catch-all response.
///
/// The body is a fixed literal: it never echoes the requested path.
pub async fn not_found() -> Response<Body> {
    not_found_response()
}

/// The non-async form, shared by the fallback and the asset miss.
pub fn not_found_response() -> Response<Body> {
    respond(
        StatusCode::NOT_FOUND,
        "text/plain; charset=utf-8",
        "not found\n".to_string(),
    )
}

/// The login form body.
#[derive(Debug, Deserialize)]
pub struct LoginForm {
    /// Operator-supplied console password.
    pub password: String,
}

/// Serves the login form.
pub async fn login_form(
    State(state): State<Arc<ConsoleState>>,
    headers: HeaderMap,
) -> Response<Body> {
    if !state.security().mode().requires_sessions() {
        // An unauthenticated console has no login to offer; sending the
        // operator to a form that cannot succeed would be misleading.
        return redirect(StatusCode::TEMPORARY_REDIRECT, "/");
    }
    // The session's CSRF token is rendered into the shell so the no-JS
    // forms can post it; the page never has to read a cookie to do so.
    let csrf = session_csrf(&state, &headers);
    let overview = build_overview(&state);
    respond(
        StatusCode::OK,
        "text/html; charset=utf-8",
        render_shell(
            state.config().theme(),
            state.config().title(),
            csrf.as_deref(),
            &overview,
        ),
    )
}

/// Verifies a password and issues a session.
pub async fn login_submit(
    State(state): State<Arc<ConsoleState>>,
    Form(form): Form<LoginForm>,
) -> Response<Body> {
    let policy = state.security();
    let now = now_secs();
    let throttle_key = policy.throttle_key();

    // Throttling is checked before the KDF so a blocked caller cannot make
    // the router spend memory on every attempt.
    if !policy.throttle().allows(throttle_key, now) {
        return respond(
            StatusCode::TOO_MANY_REQUESTS,
            "text/html; charset=utf-8",
            render_login(
                state.config().theme(),
                state.config().title(),
                Some("Too many attempts. Try again shortly."),
            ),
        );
    }

    let Some(verifier) = policy.verifier() else {
        // Unreachable: the form is only served in authenticated mode, and
        // authenticated mode always owns a verifier. Refusing is correct
        // even so.
        return respond(
            StatusCode::FORBIDDEN,
            "text/plain; charset=utf-8",
            "forbidden\n".to_string(),
        );
    };

    if !verifier.verify(&form.password) {
        policy.throttle().record_failure(throttle_key, now);
        return respond(
            StatusCode::UNAUTHORIZED,
            "text/html; charset=utf-8",
            render_login(
                state.config().theme(),
                state.config().title(),
                Some("Incorrect password."),
            ),
        );
    }

    policy.throttle().record_success(throttle_key, now);
    let (id, csrf) = match policy.sessions().create(now) {
        Ok(pair) => pair,
        Err(_) => {
            return respond(
                StatusCode::SERVICE_UNAVAILABLE,
                "text/plain; charset=utf-8",
                "too many sessions\n".to_string(),
            );
        }
    };

    let max_age = policy.session_cookie_max_age();
    let mut response = respond(
        StatusCode::SEE_OTHER,
        "text/plain; charset=utf-8",
        "signed in\n".to_string(),
    );
    with_cookie(&mut response, session_cookie_header(id.as_str(), max_age));
    with_cookie(&mut response, csrf_cookie_header(csrf.as_str(), max_age));
    response
        .headers_mut()
        .insert(header::LOCATION, HeaderValue::from_static("/"));
    response
}

/// Revokes the current session and clears both cookies.
/// Maximum accepted logout form body length.
///
/// The body carries one token and nothing else. A larger body is refused
/// before parsing rather than truncated.
pub const MAX_LOGOUT_FORM_BYTES: usize = 1024;

/// Extracts the `csrf` field from a urlencoded body.
///
/// Parsing is done here rather than with the `Form` extractor because the
/// field must be *optional*: a browser form with no session renders no
/// hidden field, and an empty body has to produce "no token", not an
/// extractor rejection with a different status.
fn logout_csrf_field(body: &str) -> String {
    if body.len() > MAX_LOGOUT_FORM_BYTES {
        return String::new();
    }
    for pair in body.split('&') {
        let Some((key, value)) = pair.split_once('=') else {
            continue;
        };
        if key.trim() == "csrf" {
            return percent_decode(value.trim());
        }
    }
    String::new()
}

/// Decodes the small percent-encoding subset a form field can carry.
///
/// `+` becomes a space and `%XX` becomes one byte. Anything malformed is
/// left literal, so a malformed token simply fails the equality check
/// instead of being silently repaired into a valid one.
fn percent_decode(value: &str) -> String {
    let bytes = value.as_bytes();
    let mut out: Vec<u8> = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        match bytes[index] {
            b'+' => {
                out.push(b' ');
                index += 1;
            }
            b'%' if index + 2 < bytes.len() => {
                let hex = &value[index + 1..index + 3];
                match u8::from_str_radix(hex, 16) {
                    Ok(decoded) => {
                        out.push(decoded);
                        index += 3;
                    }
                    Err(_) => {
                        out.push(b'%');
                        index += 1;
                    }
                }
            }
            other => {
                out.push(other);
                index += 1;
            }
        }
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// Revokes the current session and clears both cookies.
///
/// The CSRF token is verified here rather than in the guard stack because
/// it arrives as a form field, which middleware cannot read without
/// consuming the request body. The guard stack still requires a valid
/// session, so this stays a same-origin, session-bound operation, and a
/// missing or wrong token is refused without revoking anything.
pub async fn logout(
    State(state): State<Arc<ConsoleState>>,
    headers: HeaderMap,
    body: axum::body::Bytes,
) -> Response<Body> {
    let cookies = read_cookies(headers.get(header::COOKIE).and_then(|v| v.to_str().ok()));
    if let Some(session) = cookies.get(SESSION_COOKIE_NAME) {
        let supplied = logout_csrf_field(&String::from_utf8_lossy(&body));
        let valid = !supplied.is_empty()
            && state
                .security()
                .sessions()
                .verify_csrf(session, &supplied, now_secs());
        if !valid {
            return respond(
                StatusCode::FORBIDDEN,
                "text/plain; charset=utf-8",
                "forbidden\n".to_string(),
            );
        }
        state.security().sessions().revoke(session);
    }
    let mut response = respond(
        StatusCode::SEE_OTHER,
        "text/plain; charset=utf-8",
        "signed out\n".to_string(),
    );
    with_cookie(&mut response, clear_session_cookie_header());
    with_cookie(&mut response, clear_csrf_cookie_header());
    response
        .headers_mut()
        .insert(header::LOCATION, HeaderValue::from_static("/login"));
    response
}

/// Returns the bounded read-only overview as JSON.
///
/// The endpoint takes no parameters and selects no method by name: it
/// reads the fixed overview selector set through the control trait, so a
/// browser cannot widen it by sending a query string.
pub async fn overview_json(State(state): State<Arc<ConsoleState>>) -> Response<Body> {
    let overview = build_overview(&state);
    let document = overview.to_json();
    respond(
        StatusCode::OK,
        "application/json; charset=utf-8",
        format!("{document}\n"),
    )
}

/// Builds the overview view model from the installed control client.
pub fn build_overview(state: &Arc<ConsoleState>) -> Overview {
    let router_info = state.control().router_info();
    let services = state.control().client_services();
    Overview::build(&router_info, &services)
}

/// The CSRF fixture mutation.
///
/// This endpoint exists to prove that the guard stack blocks a
/// cross-site or token-less unsafe request. It mutates a counter and
/// nothing else: no router state is reachable from it.
pub async fn fixture_mark(State(state): State<Arc<ConsoleState>>) -> Response<Body> {
    let count = state.fixture().mark();
    let body = format!("{{\"marks\":{count}}}\n");
    respond(StatusCode::OK, "application/json; charset=utf-8", body)
}

/// Returns the CSRF token of the caller's valid session, if any.
fn session_csrf(state: &Arc<ConsoleState>, headers: &HeaderMap) -> Option<String> {
    let cookies = read_cookies(headers.get(header::COOKIE).and_then(|v| v.to_str().ok()));
    let session = cookies.get(SESSION_COOKIE_NAME)?;
    state
        .security()
        .sessions()
        .validate(session, now_secs())
        .ok()
        .map(|token| token.as_str().to_string())
}

/// Extracts a valid session identifier from a request's cookies.
fn extract_session(state: &Arc<ConsoleState>, headers: &HeaderMap) -> Option<String> {
    let cookies = read_cookies(headers.get(header::COOKIE).and_then(|v| v.to_str().ok()));
    let session = cookies.get(SESSION_COOKIE_NAME)?.clone();
    state
        .security()
        .sessions()
        .validate(&session, now_secs())
        .ok()?;
    Some(session)
}

/// Returns whether a request method is state-changing.
///
/// Exposed so the router's own tests can assert the property the guard
/// relies on rather than restating it.
pub fn is_unsafe_method(method: &str) -> bool {
    matches!(
        crate::security::authority::classify_method(method),
        RequestSafety::Unsafe
    )
}

/// Returns the console secret wrapper used by the daemon.
///
/// The daemon owns the plaintext password from configuration; the console
/// wraps it so the raw value cannot be logged or copied.
pub fn wrap_secret(value: &str) -> ConsoleSecret {
    ConsoleSecret::new(value)
}

/// Returns whether a response carries a CORS header.
///
/// Re-exported so router-level tests can assert the post-condition of the
/// header policy without reaching into the security module.
pub fn response_has_cors_header(response: &Response<Body>) -> bool {
    has_cors_header(response)
}

/// Returns the session cookie header name.
pub fn session_cookie_name() -> HeaderName {
    HeaderName::from_static(SESSION_COOKIE_NAME)
}

// ---------------------------------------------------------------------------
// Rendering
// ---------------------------------------------------------------------------

/// Renders the console shell document.
///
/// Plan 356/357 renders a structurally complete shell with clearly marked
/// placeholder content and no router state. Plan 358 replaces the
/// placeholder rows with Proposal-170-sourced values.
pub fn render_shell(
    theme: &ThemePalette,
    title: &str,
    csrf: Option<&str>,
    overview: &Overview,
) -> String {
    let theme_name = Text::new(&theme.name);
    let title = Text::new(title);
    let mut html = String::with_capacity(8192);
    html.push_str("<!DOCTYPE html>\n<html lang=\"en\" data-theme=\"");
    html.push_str(&theme_name.to_string());
    html.push_str("\">\n<head>\n");
    html.push_str("<meta charset=\"utf-8\">\n");
    html.push_str("<meta name=\"viewport\" content=\"width=device-width, initial-scale=1\">\n");
    html.push_str("<meta name=\"referrer\" content=\"no-referrer\">\n");
    html.push_str("<title>");
    html.push_str(&title.to_string());
    html.push_str("</title>\n");
    html.push_str("<link rel=\"stylesheet\" href=\"/assets/console.css\">\n");
    html.push_str("<link rel=\"stylesheet\" href=\"/theme.css\">\n");
    html.push_str("<script src=\"/assets/console.js\" defer></script>\n");
    html.push_str("</head>\n<body>\n");
    html.push_str("<a class=\"skip-link\" href=\"#main\">Skip to content</a>\n");

    html.push_str("<header class=\"topbar\">\n");
    html.push_str("<span class=\"brand\"><img src=\"/assets/logo.svg\" alt=\"\">");
    html.push_str("<span>i2pr</span></span>\n");
    html.push_str("<span class=\"status\"><span class=\"chip status-info\" data-console-source=\"routerinfo\">");
    html.push_str(&Text::new(overview.router_info_availability.as_str()).to_string());
    html.push_str("</span></span>\n");
    html.push_str("</header>\n");

    html.push_str("<nav class=\"nav\" aria-label=\"Sections\">\n");
    html.push_str("<a href=\"/\" aria-current=\"page\">Overview</a>\n");
    html.push_str("<form method=\"post\" action=\"/logout\" class=\"logout-form\">");
    if let Some(token) = csrf {
        html.push_str("<input type=\"hidden\" name=\"csrf\" value=\"");
        html.push_str(&Text::new(token).to_string());
        html.push_str("\">");
    }
    html.push_str("<button type=\"submit\">Sign out</button></form>\n");
    html.push_str("</nav>\n");

    html.push_str("<main id=\"main\" class=\"layout\">\n");
    html.push_str("<aside class=\"panel\">\n<h2>Router</h2>\n");
    html.push_str("<p class=\"placeholder\">Router state is not attached in this milestone.</p>\n");
    html.push_str("</aside>\n");

    html.push_str("<section class=\"panel\">\n<h2>Overview</h2>\n");
    html.push_str(
        "<div class=\"summary\" id=\"console-metrics\" data-console-refresh=\"overview\">\n",
    );
    if overview.metrics.is_empty() {
        html.push_str("<p class=\"empty\">No router metrics are available.</p>\n");
    }
    for metric in &overview.metrics {
        // A metric without a value renders an explicit unavailable marker.
        // It never renders a zero or an empty cell.
        let (value, class) = match &metric.value {
            Some(value) => (value.clone(), metric.availability.as_str()),
            None => ("\u{2014}".to_string(), "unavailable"),
        };
        html.push_str("<div class=\"metric\" data-availability=\"");
        html.push_str(metric.availability.as_str());
        html.push_str("\"><div class=\"label\">");
        html.push_str(&Text::new(metric.label).to_string());
        html.push_str("</div><div class=\"value\">");
        html.push_str(&Text::new(&value).to_string());
        html.push_str("</div><div class=\"sub\">");
        html.push_str(class);
        html.push_str("</div></div>\n");
    }
    html.push_str("</div>\n");

    html.push_str("<h2>Client services</h2>\n");
    html.push_str("<table id=\"console-services\">\n");
    html.push_str("<thead><tr><th>Service</th><th>State</th><th>Availability</th></tr></thead>\n");
    html.push_str("<tbody>\n");
    if overview.services.is_empty() {
        html.push_str("<tr><td colspan=\"3\" class=\"empty\">No client service information is available.</td></tr>\n");
    }
    for row in &overview.services {
        let state = row.state.clone().unwrap_or_else(|| "\u{2014}".to_string());
        html.push_str("<tr data-availability=\"");
        html.push_str(row.availability.as_str());
        html.push_str("\"><td>");
        html.push_str(&Text::new(&row.name).to_string());
        html.push_str("</td><td><span class=\"chip\">");
        html.push_str(&Text::new(&state).to_string());
        html.push_str("</span></td><td>");
        html.push_str(row.availability.as_str());
        html.push_str("</td></tr>\n");
    }
    html.push_str("</tbody>\n</table>\n");
    html.push_str("</section>\n");
    html.push_str("</main>\n");

    html.push_str("<footer>i2pr \u{2014} experimental router \u{2014} loopback console</footer>\n");
    html.push_str("</body>\n</html>\n");
    html
}

/// Renders the login form.
///
/// The form is same-origin and posts a single field. There is no inline
/// script and no hidden CSRF field: the CSRF token for authenticated
/// mutations lives in a cookie that the page's script attaches, and login
/// itself is protected by the same-origin check the guard stack applies.
pub fn render_login(theme: &ThemePalette, title: &str, error: Option<&str>) -> String {
    let mut html = String::with_capacity(4096);
    html.push_str("<!DOCTYPE html>\n<html lang=\"en\" data-theme=\"");
    html.push_str(&Text::new(&theme.name).to_string());
    html.push_str("\">\n<head>\n");
    html.push_str("<meta charset=\"utf-8\">\n");
    html.push_str("<meta name=\"viewport\" content=\"width=device-width, initial-scale=1\">\n");
    html.push_str("<meta name=\"referrer\" content=\"no-referrer\">\n");
    html.push_str("<title>");
    html.push_str(&Text::new(title).to_string());
    html.push_str("</title>\n");
    html.push_str("<link rel=\"stylesheet\" href=\"/assets/console.css\">\n");
    html.push_str("<link rel=\"stylesheet\" href=\"/theme.css\">\n");
    html.push_str("</head>\n<body>\n<main id=\"main\" class=\"layout\">\n");
    html.push_str("<section class=\"panel\">\n<h2>Console sign in</h2>\n");
    if let Some(message) = error {
        html.push_str("<p class=\"notice\" role=\"alert\">");
        html.push_str(&Text::new(message).to_string());
        html.push_str("</p>\n");
    }
    html.push_str("<form method=\"post\" action=\"/login\">\n");
    html.push_str("<p><label for=\"password\">Console password</label></p>\n");
    html.push_str("<p><input id=\"password\" name=\"password\" type=\"password\" autocomplete=\"current-password\" required></p>\n");
    html.push_str("<p><button type=\"submit\">Sign in</button></p>\n");
    html.push_str("</form>\n</section>\n</main>\n</body>\n</html>\n");
    html
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cookie_parsing_keeps_only_the_console_cookies() {
        let raw = "a=b; i2pr_console_session=abc; other=zzz; i2pr_console_csrf=def";
        let cookies = read_cookies(Some(raw));
        assert_eq!(
            cookies.get(SESSION_COOKIE_NAME).map(String::as_str),
            Some("abc")
        );
        assert_eq!(
            cookies.get(CSRF_COOKIE_NAME).map(String::as_str),
            Some("def")
        );
        assert_eq!(cookies.len(), 2);
    }

    #[test]
    fn cookie_parsing_keeps_the_first_duplicate() {
        // A second injected copy must not override an earlier value.
        let raw = "i2pr_console_session=first; i2pr_console_session=second";
        let cookies = read_cookies(Some(raw));
        assert_eq!(
            cookies.get(SESSION_COOKIE_NAME).map(String::as_str),
            Some("first")
        );
    }

    #[test]
    fn cookie_parsing_refuses_oversized_and_malformed_input() {
        let huge = format!(
            "i2pr_console_session={}",
            "x".repeat(crate::security::session::MAX_COOKIE_VALUE_LEN + 1)
        );
        let cookies = read_cookies(Some(&huge));
        assert!(!cookies.contains_key(SESSION_COOKIE_NAME));
        // An oversized header is discarded whole.
        assert!(read_cookies(Some(&"x".repeat(5000))).is_empty());
        assert!(read_cookies(Some("no-equals-sign")).is_empty());
        assert!(read_cookies(None).is_empty());
    }

    #[test]
    fn cookie_parsing_rejects_an_empty_value() {
        let cookies = read_cookies(Some("i2pr_console_session="));
        assert!(!cookies.contains_key(SESSION_COOKIE_NAME));
    }

    #[test]
    fn every_response_path_applies_the_security_policy() {
        let response = respond(StatusCode::OK, "text/html; charset=utf-8", "x".into());
        assert!(response.headers().contains_key("content-security-policy"));
        assert!(!response_has_cors_header(&response));

        let asset = asset_response(StatusCode::OK, "text/css; charset=utf-8", "body{}");
        assert!(asset.headers().contains_key("content-security-policy"));
        assert_eq!(
            asset
                .headers()
                .get(header::CACHE_CONTROL)
                .and_then(|v| v.to_str().ok()),
            Some("no-cache")
        );

        let not_found = not_found_response();
        assert!(not_found.headers().contains_key("content-security-policy"));
        assert_eq!(not_found.status(), StatusCode::NOT_FOUND);
    }

    #[test]
    fn shell_has_no_inline_code_so_csp_needs_no_unsafe_inline() {
        let theme = ThemePalette::resolve_or_default(crate::DEFAULT_THEME_NAME);
        for document in [
            render_shell(
                &theme,
                "t",
                None,
                &Overview::build(
                    &crate::control::ControlReply::unavailable("none"),
                    &crate::control::ControlReply::unavailable("none"),
                ),
            ),
            render_shell(
                &theme,
                "t",
                Some("a".repeat(64).leak()),
                &Overview::build(
                    &crate::control::ControlReply::unavailable("none"),
                    &crate::control::ControlReply::unavailable("none"),
                ),
            ),
            render_login(&theme, "t", None),
        ] {
            assert!(!document.contains("<style"));
            assert!(!document.contains("style="));
            assert!(!document.contains("<script>"));
            assert!(!document.contains("onclick"));
            assert!(!document.contains("onload"));
        }
    }

    #[test]
    fn login_error_text_is_escaped() {
        let theme = ThemePalette::resolve_or_default(crate::DEFAULT_THEME_NAME);
        let rendered = render_login(&theme, "t", Some("<script>alert(1)</script>"));
        assert!(!rendered.contains("<script>alert"));
        assert!(rendered.contains("&lt;script&gt;"));
    }

    #[test]
    fn login_form_posts_only_the_password_to_a_same_origin_action() {
        let theme = ThemePalette::resolve_or_default(crate::DEFAULT_THEME_NAME);
        let rendered = render_login(&theme, "t", None);
        assert!(rendered.contains("action=\"/login\""));
        assert!(rendered.contains("name=\"password\""));
        assert!(rendered.contains("type=\"password\""));
        assert!(!rendered.contains("name=\"token\""));
    }

    #[test]
    fn unsafe_method_classification_matches_the_guard() {
        for method in ["POST", "PUT", "PATCH", "DELETE"] {
            assert!(is_unsafe_method(method), "{method}");
        }
        for method in ["GET", "HEAD"] {
            assert!(!is_unsafe_method(method), "{method}");
        }
    }

    #[test]
    fn wrapped_secret_is_redacted() {
        assert!(!format!("{:?}", wrap_secret("hunter2")).contains("hunter2"));
    }
}
