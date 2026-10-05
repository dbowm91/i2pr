//! The console browser-security guard stack.
//!
//! Plan 357. The stack is applied in a fixed order, before any route
//! handler runs, so no handler has to remember to re-check anything:
//!
//! 1. syntactic bounds on the authority/origin headers;
//! 2. `Host`/authority validation (DNS-rebinding defence);
//! 3. origin validation for unsafe methods (cross-site request defence);
//! 4. session authentication, when the console is in authenticated mode;
//! 5. CSRF validation for unsafe authenticated actions;
//! 6. the route handler.
//!
//! Every response, including every rejection, passes through the centralized
//! header policy in [`headers`]. CORS is never emitted.
//!
//! Modes. An unauthenticated console is *not* a weaker version of an
//! authenticated one: it still validates authority and origin, still
//! applies the full header policy, and contains no dormant default
//! password or pre-minted session. Authentication adds a credential check
//! and a session table; it never *removes* a check.

pub mod auth;
pub mod authority;
pub mod headers;
pub mod session;

use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;

use auth::{LoginThrottle, PasswordVerifier};
use authority::{AuthorityPolicy, RequestSafety, classify_method};

use crate::secret::ConsoleSecret;
pub use session::{SessionLimits, SessionStore};

/// Whether the console requires a browser login.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AuthMode {
    /// Loopback-only, no credential. Still authority- and origin-checked.
    Disabled,
    /// A console password is required; sessions are issued on success.
    Password,
}

impl AuthMode {
    /// Returns whether this mode issues and validates sessions.
    pub const fn requires_sessions(self) -> bool {
        matches!(self, Self::Password)
    }
}

/// Configurable throttle window, in seconds.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ThrottleLimits {
    /// Failed attempts allowed inside the window.
    pub max_failures: u32,
    /// Window length in seconds.
    pub window_secs: u64,
    /// Entries retained for distinct peers.
    pub max_peers: usize,
}

impl Default for ThrottleLimits {
    fn default() -> Self {
        Self {
            max_failures: 5,
            window_secs: 5 * 60,
            max_peers: 256,
        }
    }
}

/// The complete, validated console security policy.
#[derive(Clone, Debug)]
pub struct SecurityPolicy {
    mode: AuthMode,
    authority: AuthorityPolicy,
    sessions: Arc<SessionStore>,
    verifier: Option<Arc<PasswordVerifier>>,
    throttle: Arc<LoginThrottle>,
}

impl SecurityPolicy {
    /// Builds an unauthenticated policy for a loopback listener.
    pub fn unauthenticated(
        bound: SocketAddr,
        session_limits: SessionLimits,
        throttle: ThrottleLimits,
    ) -> Result<Self, SecurityError> {
        Ok(Self {
            mode: AuthMode::Disabled,
            authority: AuthorityPolicy::for_listener(bound)?,
            sessions: Arc::new(SessionStore::new(session_limits)),
            // No verifier exists in disabled mode: there is no dormant
            // credential to be reached by flipping a flag.
            verifier: None,
            throttle: Arc::new(LoginThrottle::new(
                throttle.max_failures as usize,
                throttle.window_secs,
                throttle.max_peers,
            )),
        })
    }

    /// Builds an authenticated policy for a loopback listener.
    ///
    /// `password` is the operator-supplied plaintext; it is converted to an
    /// Argon2id verifier here and the raw value is dropped before this
    /// function returns. An empty password is refused rather than accepted.
    pub fn authenticated(
        bound: SocketAddr,
        password: ConsoleSecret,
        session_limits: SessionLimits,
        throttle: ThrottleLimits,
    ) -> Result<Self, SecurityError> {
        let mut derived = auth::derive_from_password(password)?;
        derived.forget_temporary();
        Ok(Self {
            mode: AuthMode::Password,
            authority: AuthorityPolicy::for_listener(bound)?,
            sessions: Arc::new(SessionStore::new(session_limits)),
            verifier: Some(derived.verifier()),
            throttle: Arc::new(LoginThrottle::new(
                throttle.max_failures as usize,
                throttle.window_secs,
                throttle.max_peers,
            )),
        })
    }

    /// Builds an authenticated policy from a pre-hashed PHC verifier.
    pub fn authenticated_from_hash(
        bound: SocketAddr,
        hash: &str,
        session_limits: SessionLimits,
        throttle: ThrottleLimits,
    ) -> Result<Self, SecurityError> {
        let verifier = auth::verifier_from_hash(hash)?;
        Ok(Self {
            mode: AuthMode::Password,
            authority: AuthorityPolicy::for_listener(bound)?,
            sessions: Arc::new(SessionStore::new(session_limits)),
            verifier: Some(Arc::new(verifier)),
            throttle: Arc::new(LoginThrottle::new(
                throttle.max_failures as usize,
                throttle.window_secs,
                throttle.max_peers,
            )),
        })
    }

    /// Returns the authentication mode.
    pub fn mode(&self) -> AuthMode {
        self.mode
    }

    /// Returns the authority policy.
    pub fn authority(&self) -> &AuthorityPolicy {
        &self.authority
    }

    /// Returns the session store.
    pub fn sessions(&self) -> &Arc<SessionStore> {
        &self.sessions
    }

    /// Returns the password verifier, if this mode owns one.
    pub fn verifier(&self) -> Option<&Arc<PasswordVerifier>> {
        self.verifier.as_ref()
    }

    /// Returns the key login attempts are throttled under.
    ///
    /// The console binds one loopback listener and, by design, trusts no
    /// proxy header, and the HTTP substrate does not surface a per-connection
    /// peer identity to the application. There is therefore no unforgeable
    /// per-client key to throttle on, and inventing one from `Host` or a
    /// forwarded header would be a guess an attacker controls.
    ///
    /// The throttle is consequently console-wide. Its failure mode is a
    /// bounded login *delay* that expires with the window, never a permanent
    /// lockout, and it still stops a local brute-force loop from making the
    /// router spend KDF memory on every attempt.
    pub fn throttle_key(&self) -> &'static str {
        "loopback-console"
    }

    /// Returns the login throttle.
    pub fn throttle(&self) -> &Arc<LoginThrottle> {
        &self.throttle
    }

    /// Returns the session requirement for a path under this policy.
    ///
    /// Unauthenticated consoles require no session on any path. An
    /// authenticated console protects everything except the login/logout
    /// endpoints, the shell entry point, the generated theme, and the
    /// compiled assets: gating those would render the login page itself
    /// unstyled and unreachable.
    pub fn requirement_for(policy: &SecurityPolicy, path: &str) -> SessionRequirement {
        if !policy.mode.requires_sessions() {
            return SessionRequirement::NotRequired;
        }
        if is_public_path(path) || is_asset_path(path) {
            return SessionRequirement::NotRequired;
        }
        SessionRequirement::Required
    }

    /// Returns the maximum cookie lifetime in seconds.
    pub fn session_cookie_max_age(&self) -> u64 {
        self.sessions.limits().absolute_timeout.as_secs()
    }

    /// Returns the absolute session lifetime.
    pub fn session_absolute_timeout(&self) -> Duration {
        self.sessions.limits().absolute_timeout
    }

    /// Drops all authentication state. Used on shutdown.
    pub fn shutdown(&self) {
        self.sessions.clear();
    }
}

/// Why a security policy could not be built.
#[derive(Clone, Debug)]
pub enum SecurityError {
    /// The listener was not loopback.
    Authority(authority::AuthorityError),
    /// The credential could not be turned into a verifier.
    Password(auth::PasswordError),
}

impl From<authority::AuthorityError> for SecurityError {
    fn from(error: authority::AuthorityError) -> Self {
        Self::Authority(error)
    }
}

impl From<auth::PasswordError> for SecurityError {
    fn from(error: auth::PasswordError) -> Self {
        Self::Password(error)
    }
}

impl std::fmt::Display for SecurityError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Authority(error) => write!(formatter, "console authority: {error}"),
            Self::Password(error) => write!(formatter, "console password: {error}"),
        }
    }
}

impl std::error::Error for SecurityError {}

/// The visible outcome of running the guard stack.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum GuardOutcome {
    /// The request may proceed to the handler.
    Allow,
    /// The request was refused; no handler ran.
    Refused,
}

/// Whether a request must carry a valid console session.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SessionRequirement {
    /// The path is reachable without a credential (login, assets, shell).
    NotRequired,
    /// The path is protected.
    Required,
}

/// A single, auditable decision about one request.
///
/// The guard is a pure function of the request's headers, the method, and
/// the policy, so its behaviour is testable without a router and cannot
/// drift between call sites.
#[derive(Clone, Copy, Debug)]
pub struct RequestGuard<'a> {
    method: &'a str,
    host: Option<&'a str>,
    origin: Option<&'a str>,
    referer: Option<&'a str>,
    session_cookie: Option<&'a str>,
    csrf_cookie: Option<&'a str>,
}

impl<'a> RequestGuard<'a> {
    /// Builds a guard from the relevant request headers.
    pub fn new(
        method: &'a str,
        host: Option<&'a str>,
        origin: Option<&'a str>,
        referer: Option<&'a str>,
        session_cookie: Option<&'a str>,
        csrf_cookie: Option<&'a str>,
    ) -> Self {
        Self {
            method,
            host,
            origin,
            referer,
            session_cookie,
            csrf_cookie,
        }
    }

    /// Returns the request's safety class.
    pub fn safety(&self) -> RequestSafety {
        classify_method(self.method)
    }

    /// Runs steps 1 through 5 of the guard stack.
    ///
    /// Returns `true` when the request may reach a handler.
    pub fn admit(
        &self,
        policy: &SecurityPolicy,
        now_secs: u64,
        requirement: SessionRequirement,
        path: &str,
    ) -> bool {
        // Step 2: effective authority. This runs for every method, in both
        // authenticated and unauthenticated mode, because DNS rebinding
        // does not care whether a password is set.
        let Some(host) = self.host else {
            return false;
        };
        if !policy.authority.accepts_host(host) {
            return false;
        }

        // Step 3: cross-origin policy for unsafe methods.
        if self.safety() == RequestSafety::Unsafe && !self.same_origin(policy) {
            return false;
        }

        // Steps 4 and 5 apply only where a session is both required and
        // possible. An unauthenticated console has no session table to
        // consult, so this check is a no-op there by construction.
        if requirement == SessionRequirement::NotRequired || !policy.mode.requires_sessions() {
            return true;
        }

        // A request with no session cookie cannot be authenticated.
        let Some(cookie) = self.session_cookie else {
            return false;
        };
        if cookie.is_empty() || cookie.len() > session::MAX_COOKIE_VALUE_LEN {
            return false;
        }
        match policy.sessions.validate(cookie, now_secs) {
            Ok(_) => {}
            Err(_) => return false,
        }
        if self.safety() == RequestSafety::Unsafe && !csrf_exempt_path(path) {
            // CSRF is checked in addition to same-origin, never instead of
            // it: `SameSite=Strict` is a useful default, not a guarantee.
            let Some(csrf) = self.csrf_cookie else {
                return false;
            };
            return policy.sessions.verify_csrf(cookie, csrf, now_secs);
        }
        true
    }

    /// Returns whether an unsafe request is same-origin.
    ///
    /// A browser always sends `Origin` on a cross-origin unsafe request, so
    /// a *present* `Origin` must match. `Referer` is consulted only when
    /// `Origin` is absent, which is the single case where the browser
    /// withholds it.
    fn same_origin(&self, policy: &SecurityPolicy) -> bool {
        match self.origin {
            Some(origin) if !origin.is_empty() => policy.authority.accepts_origin(origin),
            _ => match self.referer {
                Some(referer) if !referer.is_empty() => policy.authority.accepts_referer(referer),
                // Neither header present on an unsafe request: refuse. The
                // default is closed, not open.
                _ => false,
            },
        }
    }
}

/// Whether a path is reachable without a session while authenticated.
///
/// Login and logout must be reachable without a session, otherwise an
/// operator can never authenticate or revoke. Everything else is
/// protected.
pub fn is_public_path(path: &str) -> bool {
    matches!(path, "/" | "/login" | "/theme.css")
}

/// Returns whether the middleware CSRF step is skipped for a path.
///
/// There is exactly one exemption, and it is not a loophole: `/logout`
/// carries its CSRF token as a **form field**, which middleware cannot read
/// without consuming the request body. The `logout` handler performs the
/// same `verify_csrf` check itself, so the token is still mandatory and the
/// operation is still bound to the caller's own session. Skipping it at the
/// cookie layer would instead break the server-rendered, script-free logout
/// form, which is the accessibility-friendly path the console must keep.
pub fn csrf_exempt_path(path: &str) -> bool {
    path == "/logout"
}

/// Returns whether an asset path is reachable without a session.
///
/// Compiled assets carry no state, so gating them behind a login would only
/// produce a broken-looking login page.
pub fn is_asset_path(path: &str) -> bool {
    path.starts_with("/assets/")
}

/// Returns whether a path may present console cookies.
///
/// Invariant 12 of Plan 357: a future eepsite listener must not be able to
/// reuse the console's routes or cookies. The console owns a disjoint path
/// space, and this predicate is the single statement of that fact.
pub fn owns_path(path: &str) -> bool {
    path == "/"
        || path == "/login"
        || path == "/logout"
        || path == "/theme.css"
        || is_asset_path(path)
        || path.starts_with("/api/")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::{IpAddr, Ipv4Addr};

    fn bound() -> SocketAddr {
        SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 7070)
    }

    fn open_policy() -> SecurityPolicy {
        SecurityPolicy::unauthenticated(
            bound(),
            SessionLimits::default(),
            ThrottleLimits::default(),
        )
        .expect("policy builds")
    }

    fn locked_policy() -> SecurityPolicy {
        SecurityPolicy::authenticated(
            bound(),
            ConsoleSecret::new("correct password"),
            SessionLimits::default(),
            ThrottleLimits::default(),
        )
        .expect("policy builds")
    }

    fn get(host: &'static str) -> RequestGuard<'static> {
        RequestGuard::new("GET", Some(host), None, None, None, None)
    }

    fn post(host: &'static str, origin: Option<&'static str>) -> RequestGuard<'static> {
        RequestGuard::new("POST", Some(host), origin, None, None, None)
    }

    #[test]
    fn host_validation_applies_to_every_method_in_both_modes() {
        // Unauthenticated mode: authority alone decides a GET.
        let open = open_policy();
        assert!(get("localhost:7070").admit(&open, 0, SessionRequirement::Required, "/"));
        for hostile in [
            "evil.test:7070",
            "localhost.evil.test:7070",
            "127.0.0.1:9999",
        ] {
            assert!(
                !get(hostile).admit(&open, 0, SessionRequirement::Required, "/"),
                "{hostile} must be refused"
            );
        }

        // Authenticated mode: authority is checked first and identically.
        // A valid session is supplied so the refusal is attributable to
        // the host check rather than to a missing credential.
        let locked = locked_policy();
        let (id, csrf) = locked.sessions().create(0).expect("session created");
        let authenticated = |host: &'static str| {
            RequestGuard::new(
                "GET",
                Some(host),
                None,
                None,
                Some(id.as_str()),
                Some(csrf.as_str()),
            )
            .admit(&locked, 1, SessionRequirement::Required, "/")
        };
        assert!(authenticated("localhost:7070"));
        assert!(!authenticated("evil.test:7070"));
        assert!(!authenticated("localhost.evil.test:7070"));

        // A missing Host header is refused in both modes, never defaulted.
        for policy in [open_policy(), locked_policy()] {
            let no_host = RequestGuard::new("GET", None, None, None, None, None);
            assert!(!no_host.admit(&policy, 0, SessionRequirement::Required, "/"));
        }
    }

    #[test]
    fn safe_requests_ignore_origin_but_unsafe_ones_do_not() {
        let policy = open_policy();
        // GET carries no origin and is still admitted.
        assert!(get("localhost:7070").admit(&policy, 0, SessionRequirement::Required, "/"));
        // A cross-origin POST is refused before any handler runs.
        assert!(!post("localhost:7070", Some("http://evil.test")).admit(
            &policy,
            0,
            SessionRequirement::Required,
            "/"
        ));
        // A same-origin POST is admitted.
        assert!(post("localhost:7070", Some("http://localhost:7070")).admit(
            &policy,
            0,
            SessionRequirement::Required,
            "/"
        ));
    }

    #[test]
    fn unsafe_request_without_origin_or_referer_is_refused_by_default() {
        let policy = open_policy();
        assert!(!post("localhost:7070", None).admit(&policy, 0, SessionRequirement::Required, "/"));
    }

    #[test]
    fn referer_is_a_conservative_fallback_only() {
        let policy = open_policy();
        let with_referer = RequestGuard::new(
            "POST",
            Some("localhost:7070"),
            None,
            Some("http://localhost:7070/login"),
            None,
            None,
        );
        assert!(with_referer.admit(&policy, 0, SessionRequirement::Required, "/"));
        let evil_referer = RequestGuard::new(
            "POST",
            Some("localhost:7070"),
            None,
            Some("http://evil.test/"),
            None,
            None,
        );
        assert!(!evil_referer.admit(&policy, 0, SessionRequirement::Required, "/"));
        // A present Origin wins over a benign Referer.
        let conflicting = RequestGuard::new(
            "POST",
            Some("localhost:7070"),
            Some("http://evil.test"),
            Some("http://localhost:7070/"),
            None,
            None,
        );
        assert!(!conflicting.admit(&policy, 0, SessionRequirement::Required, "/"));
    }

    #[test]
    fn unauthenticated_mode_admits_without_any_credential() {
        let policy = open_policy();
        assert_eq!(policy.mode(), AuthMode::Disabled);
        assert!(get("localhost:7070").admit(&policy, 0, SessionRequirement::Required, "/"));
        // And it holds no credential to be reached by accident.
        assert!(policy.verifier.is_none());
        assert!(policy.sessions().is_empty());
    }

    #[test]
    fn authenticated_mode_refuses_a_request_without_a_session() {
        let policy = locked_policy();
        assert_eq!(policy.mode(), AuthMode::Password);
        assert!(policy.mode().requires_sessions());
        // No cookie at all: refused.
        let anonymous = RequestGuard::new("GET", Some("localhost:7070"), None, None, None, None);
        assert!(!anonymous.admit(&policy, 0, SessionRequirement::Required, "/"));
    }

    #[test]
    fn authenticated_mode_admits_a_valid_session_and_refuses_a_forged_one() {
        let policy = locked_policy();
        let (id, csrf) = policy.sessions().create(0).expect("session created");
        let authenticated = RequestGuard::new(
            "GET",
            Some("localhost:7070"),
            None,
            None,
            Some(id.as_str()),
            Some(csrf.as_str()),
        );
        assert!(authenticated.admit(&policy, 1, SessionRequirement::Required, "/"));

        // A wrong CSRF token blocks the unsafe request even though the
        // session cookie is valid.
        let forged_csrf = RequestGuard::new(
            "POST",
            Some("localhost:7070"),
            Some("http://localhost:7070"),
            None,
            Some(id.as_str()),
            Some("0000000000000000000000000000000000000000000000000000000000000000"),
        );
        assert!(!forged_csrf.admit(&policy, 1, SessionRequirement::Required, "/"));

        // An unknown session identifier is refused.
        let unknown_session = RequestGuard::new(
            "GET",
            Some("localhost:7070"),
            None,
            None,
            Some("ffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff"),
            Some(csrf.as_str()),
        );
        assert!(!unknown_session.admit(&policy, 1, SessionRequirement::Required, "/"));
    }

    #[test]
    fn revoked_and_expired_sessions_are_refused() {
        let policy = locked_policy();
        let (id, csrf) = policy.sessions().create(0).expect("session created");
        let request = |now: u64| {
            RequestGuard::new(
                "GET",
                Some("localhost:7070"),
                None,
                None,
                Some(id.as_str()),
                Some(csrf.as_str()),
            )
            .admit(&policy, now, SessionRequirement::Required, "/")
        };
        assert!(request(1));
        policy.sessions().revoke(id.as_str());
        assert!(!request(2), "a revoked session must be refused");

        let (expiring, expiring_csrf) = policy.sessions().create(0).expect("session created");
        let lifetime = policy.session_absolute_timeout().as_secs();
        let expiring_request = RequestGuard::new(
            "GET",
            Some("localhost:7070"),
            None,
            None,
            Some(expiring.as_str()),
            Some(expiring_csrf.as_str()),
        );
        assert!(expiring_request.admit(&policy, 1, SessionRequirement::Required, "/"));
        assert!(!expiring_request.admit(&policy, lifetime + 1, SessionRequirement::Required, "/"));
    }

    #[test]
    fn shutdown_drops_all_authentication_state() {
        let policy = locked_policy();
        policy.sessions().create(0).expect("session created");
        assert_eq!(policy.sessions().len(), 1);
        policy.shutdown();
        assert!(policy.sessions().is_empty());
    }

    #[test]
    fn empty_password_is_refused_rather_than_accepted() {
        let result = SecurityPolicy::authenticated(
            bound(),
            ConsoleSecret::new(""),
            SessionLimits::default(),
            ThrottleLimits::default(),
        );
        assert!(
            result.is_err(),
            "auth = true with an empty password must fail"
        );
    }

    #[test]
    fn non_loopback_listener_is_refused_in_both_modes() {
        let remote = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(192, 168, 1, 5)), 7070);
        assert!(
            SecurityPolicy::unauthenticated(
                remote,
                SessionLimits::default(),
                ThrottleLimits::default()
            )
            .is_err()
        );
        assert!(
            SecurityPolicy::authenticated(
                remote,
                ConsoleSecret::new("password"),
                SessionLimits::default(),
                ThrottleLimits::default()
            )
            .is_err(),
            "authentication must never permit a non-loopback bind"
        );
    }

    #[test]
    fn requirement_mapping_protects_the_api_surface_only() {
        let policy = locked_policy();
        for path in ["/api/overview", "/api/fixture/mark", "/logout/extra"] {
            assert_eq!(
                SecurityPolicy::requirement_for(&policy, path),
                SessionRequirement::Required,
                "{path} must require a session"
            );
        }
        for path in ["/", "/login", "/theme.css", "/assets/console.css"] {
            assert_eq!(
                SecurityPolicy::requirement_for(&policy, path),
                SessionRequirement::NotRequired,
                "{path} must stay reachable so the login page renders"
            );
        }
        // An unauthenticated console protects nothing.
        let open = open_policy();
        assert_eq!(
            SecurityPolicy::requirement_for(&open, "/api/overview"),
            SessionRequirement::NotRequired
        );
    }

    #[test]
    fn console_path_space_is_disjoint_from_any_other_listener() {
        assert!(owns_path("/"));
        assert!(owns_path("/login"));
        assert!(owns_path("/assets/console.css"));
        assert!(owns_path("/api/overview"));
        // A non-console path is not claimed by the console, which is what
        // stops a future eepsite listener from inheriting console cookies.
        assert!(!owns_path("/index.html"));
        assert!(!owns_path("/eepsite/"));
        assert!(!is_asset_path("/assetsx/console.css"));
    }

    #[test]
    fn public_paths_do_not_include_the_api_surface() {
        assert!(is_public_path("/"));
        assert!(is_public_path("/login"));
        assert!(is_public_path("/theme.css"));
        assert!(!is_public_path("/api/overview"));
    }
}
