//! Console construction: configuration, shared state, and the router.
//!
//! The console owns the browser-facing application. It does not own a
//! socket: `i2pr-daemon` binds the loopback listener and hands the router
//! to the EggServe server adapter, so no `axum::serve` path exists here.
//!
//! The security policy is a constructor argument rather than a field of
//! [`ConsoleConfig`] because it depends on the resolved bind address. An
//! authority policy cannot be right before the port is known, and guessing
//! one would let a console answer on an authority the operator never bound.

use std::sync::Arc;

use axum::Router;

pub mod assets;
pub mod color;
pub mod control;
pub mod html;
pub mod routes;
pub mod secret;
pub mod security;
pub mod theme;

pub use control::{Availability, ControlClient, ControlReply, Overview, UnavailableControlClient};
pub use secret::ConsoleSecret;
pub use security::{AuthMode, SecurityError, SecurityPolicy, SessionLimits, ThrottleLimits};
pub use theme::{DEFAULT_THEME_NAME, ThemeName, ThemePalette};

/// The console router type handed to the daemon's server adapter.
///
/// The alias exists so the daemon never names an `axum` type directly and
/// the substrate boundary is a single, checkable seam.
pub type AppRouter = Router;

/// Maximum accepted length of the browser document title.
pub const MAX_TITLE_LEN: usize = 120;

/// Validated console configuration.
///
/// Both fields are already validated: the theme is a resolved palette and
/// the title is length-bounded. Constructing a `ConsoleConfig` therefore
/// cannot produce a console that fails at request time.
#[derive(Clone, Debug)]
pub struct ConsoleConfig {
    theme: ThemePalette,
    title: String,
}

impl ConsoleConfig {
    /// Builds a configuration for a bundled theme name.
    ///
    /// An unknown or oversized name falls back to the compiled default
    /// palette rather than erroring, so a stale configuration file degrades
    /// to a readable console instead of a failing listener.
    pub fn new(theme_name: &str) -> Self {
        Self {
            theme: ThemePalette::resolve_or_default(theme_name),
            title: default_title(),
        }
    }

    /// Returns the resolved palette.
    pub fn theme(&self) -> &ThemePalette {
        &self.theme
    }

    /// Returns the browser document title.
    pub fn title(&self) -> &str {
        &self.title
    }

    /// Returns the validated theme identifier.
    pub fn theme_name(&self) -> ThemeName<'_> {
        // `ThemePalette::resolve_or_default` only ever stores a name from
        // the compiled inventory, so this revalidation cannot fail.
        ThemeName::resolve(&self.theme.name).unwrap_or(
            ThemeName::resolve(theme::DEFAULT_THEME_NAME)
                .expect("the compiled default name is always resolvable"),
        )
    }
}

fn default_title() -> String {
    "i2pr router console".to_string()
}

/// A mutable value used only by the CSRF fixture route.
///
/// Plan 357 proves the CSRF mechanism with a fixture mutation rather than a
/// real router action. Keeping the marker in its own type makes it obvious
/// at a glance that no router state is reachable from it.
#[derive(Clone, Debug, Default)]
pub struct FixtureMarker {
    marks: Arc<std::sync::atomic::AtomicUsize>,
}

impl FixtureMarker {
    /// Returns the current mark count.
    pub fn count(&self) -> usize {
        self.marks.load(std::sync::atomic::Ordering::SeqCst)
    }

    /// Records one authenticated mutation and returns the new count.
    pub fn mark(&self) -> usize {
        self.marks.fetch_add(1, std::sync::atomic::Ordering::SeqCst) + 1
    }
}

/// Shared state handed to every console handler.
#[derive(Clone, Debug)]
pub struct ConsoleState {
    config: Arc<ConsoleConfig>,
    security: Arc<SecurityPolicy>,
    control: Arc<dyn ControlClient>,
    fixture: FixtureMarker,
}

impl ConsoleState {
    /// Builds state with no control client.
    ///
    /// The console still renders; every overview field reports itself
    /// unavailable rather than showing zeros.
    pub fn new(config: ConsoleConfig, security: SecurityPolicy) -> Self {
        Self::with_control(config, security, Arc::new(UnavailableControlClient))
    }

    /// Builds state around a read-only control client.
    pub fn with_control(
        config: ConsoleConfig,
        security: SecurityPolicy,
        control: Arc<dyn ControlClient>,
    ) -> Self {
        Self {
            config: Arc::new(config),
            security: Arc::new(security),
            control,
            fixture: FixtureMarker::default(),
        }
    }

    /// Returns the read-only control client.
    pub fn control(&self) -> &Arc<dyn ControlClient> {
        &self.control
    }

    /// Returns the shared configuration.
    pub fn config(&self) -> &ConsoleConfig {
        &self.config
    }

    /// Returns the security policy.
    pub fn security(&self) -> &SecurityPolicy {
        &self.security
    }

    /// Returns the CSRF fixture marker.
    pub fn fixture(&self) -> &FixtureMarker {
        &self.fixture
    }
}

/// Builds the hardened console router.
///
/// This is the only supported entry point: it applies the centralized
/// security-header policy and the ordered guard stack to every route.
pub fn router(state: Arc<ConsoleState>) -> AppRouter {
    routes::build(state)
}

/// Builds the router without the guard stack.
///
/// Plan 357 tests the security policy as a pure function and separately
/// proves the layered router end to end. This constructor exists so the
/// unguarded surface is one obvious call, not a router assembled
/// incrementally across several sites.
pub fn bare_router(state: Arc<ConsoleState>) -> AppRouter {
    routes::bare(state)
}

/// Current wall-clock seconds since the Unix epoch.
///
/// Expiry and throttle comparisons use this. Tests pass explicit values
/// instead, so time-dependent behaviour is exercised deterministically.
pub fn now_secs() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|elapsed| elapsed.as_secs())
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::{IpAddr, Ipv4Addr, SocketAddr};

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

    #[test]
    fn unknown_theme_falls_back_to_a_readable_default() {
        let config = ConsoleConfig::new("no-such-theme");
        assert!(config.theme().readability_failures().is_empty());
        assert_eq!(config.theme().name, theme::DEFAULT_THEME_NAME);
        assert!(config.theme_name().as_str().len() <= MAX_TITLE_LEN);
    }

    #[test]
    fn known_theme_is_used() {
        let config = ConsoleConfig::new(theme::DEFAULT_THEME_NAME);
        assert_eq!(config.theme().name, theme::DEFAULT_THEME_NAME);
        assert_eq!(config.title(), "i2pr router console");
    }

    #[test]
    fn unauthenticated_state_holds_no_credential() {
        let state = ConsoleState::new(ConsoleConfig::new(theme::DEFAULT_THEME_NAME), open_policy());
        assert_eq!(state.security().mode(), AuthMode::Disabled);
        assert!(state.security().sessions().is_empty());
    }

    #[test]
    fn fixture_marker_counts_monotonically() {
        let marker = FixtureMarker::default();
        assert_eq!(marker.count(), 0);
        assert_eq!(marker.mark(), 1);
        assert_eq!(marker.mark(), 2);
        assert_eq!(marker.count(), 2);
    }

    #[test]
    fn now_secs_is_after_the_epoch() {
        assert!(now_secs() > 1_700_000_000);
    }
}
