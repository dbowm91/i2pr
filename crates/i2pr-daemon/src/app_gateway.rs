//! Trusted, per-app-instance router access to private SAM and I2CP connections.
//!
//! This module is infrastructure for a future managed-app runtime. It does
//! not authenticate app processes or decode managed-app frames; trusted
//! composition supplies the principal and effective grants.

// No production app-runtime caller exists yet; this infrastructure API is
// exercised by its module tests until a separately planned consumer lands.
#![allow(dead_code)]

use crate::addressbook::SharedAddressBook;
use crate::config::{I2cpConfig, SamConfig};
use crate::i2cp::{I2cpPrivateConnectionError, I2cpServiceError, I2cpServiceState};
use crate::sam::{PrivateConnectionAdmissionError, SamServiceError, SamServiceState};
use i2pr_app_proto::{AppPrincipal, AppService, Capability, EffectiveCapabilities, MAX_STREAMS};
use i2pr_runtime::{CancellationToken, ChildScope, ChildTaskFailure};
use std::sync::Arc;
use thiserror::Error;
use tokio::sync::{Mutex, OwnedSemaphorePermit, Semaphore, oneshot};

const MAX_GATEWAY_CONNECTIONS: usize = MAX_STREAMS;

/// Immutable authority obtained only from trusted daemon composition.
///
/// This deliberately does not implement serde traits. In particular, an app's
/// deserialized `hello` and `AppPrincipal` value cannot construct it.
#[derive(Clone, Debug)]
pub(crate) struct AppGatewayAuthorization {
    principal: AppPrincipal,
    effective_capabilities: EffectiveCapabilities,
}

impl AppGatewayAuthorization {
    /// Constructs authorization after the trusted runtime has authenticated
    /// the app instance and projected administrator grants.
    pub(crate) fn from_trusted_composition(
        principal: AppPrincipal,
        effective_capabilities: EffectiveCapabilities,
    ) -> Self {
        Self {
            principal,
            effective_capabilities,
        }
    }

    pub(crate) fn principal(&self) -> &AppPrincipal {
        &self.principal
    }
}

/// Per-launch limits. The protocol ceiling is 128 live logical streams.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct AppGatewayLimits {
    max_connections: usize,
}

impl AppGatewayLimits {
    pub(crate) fn new(max_connections: usize) -> Result<Self, AppGatewayError> {
        if max_connections == 0 || max_connections > MAX_GATEWAY_CONNECTIONS {
            return Err(AppGatewayError::InvalidLimits);
        }
        Ok(Self { max_connections })
    }
}

/// Inputs owned by daemon composition for one app launch instance.
pub(crate) struct AppGatewayComposition {
    pub(crate) sam: SamConfig,
    pub(crate) sam_max_version: i2pr_api::sam::version::SamVersion,
    pub(crate) i2cp: I2cpConfig,
    pub(crate) addressbook: SharedAddressBook,
    /// Dedicated to this gateway session and parented by its daemon owner.
    pub(crate) children: ChildScope,
    /// Parent token for `children`, retained so session drop cancels the scope.
    pub(crate) cancellation: CancellationToken,
}

/// One app-instance authority domain and its private protocol owners.
pub(crate) struct AppGatewaySession {
    authorization: AppGatewayAuthorization,
    limits: AppGatewayLimits,
    sam_config: SamConfig,
    sam_max_version: i2pr_api::sam::version::SamVersion,
    i2cp_config: I2cpConfig,
    addressbook: SharedAddressBook,
    sam: Mutex<Option<Arc<SamServiceState>>>,
    i2cp: Mutex<Option<Arc<I2cpServiceState>>>,
    connections: Arc<Semaphore>,
    children: ChildScope,
    cancellation: CancellationToken,
}

impl AppGatewaySession {
    pub(crate) fn new(
        authorization: AppGatewayAuthorization,
        limits: AppGatewayLimits,
        composition: AppGatewayComposition,
    ) -> Self {
        let mut sam_config = composition.sam;
        let mut i2cp_config = composition.i2cp;
        // Private drivers never bind listeners. Keep their configurations
        // explicitly listener-disabled even if the daemon listener is active.
        sam_config.enabled = false;
        i2cp_config.enabled = false;
        let max_connections = limits
            .max_connections
            .min(usize::from(sam_config.limits.max_clients))
            .min(usize::from(i2cp_config.max_clients));
        let limits = AppGatewayLimits { max_connections };
        Self {
            authorization,
            limits,
            sam_config,
            sam_max_version: composition.sam_max_version,
            i2cp_config,
            addressbook: composition.addressbook,
            sam: Mutex::new(None),
            i2cp: Mutex::new(None),
            connections: Arc::new(Semaphore::new(limits.max_connections)),
            children: composition.children,
            cancellation: composition.cancellation,
        }
    }

    pub(crate) fn principal(&self) -> &AppPrincipal {
        self.authorization.principal()
    }

    /// Performs only the exact capability check. This is intentionally
    /// side-effect free and can be used for the reserved service response.
    pub(crate) fn authorize(&self, service: AppService) -> Result<(), AppGatewayError> {
        let required = match service {
            AppService::Sam => Capability::Sam,
            AppService::I2cp => Capability::I2cp,
            AppService::ControlScoped => return Err(AppGatewayError::UnsupportedService),
        };
        if self
            .authorization
            .effective_capabilities
            .as_slice()
            .contains(&required)
        {
            Ok(())
        } else {
            Err(AppGatewayError::PermissionDenied)
        }
    }

    /// Opens one private SAM protocol connection over the caller-owned byte
    /// stream. No TCP listener or loopback connection is involved.
    pub(crate) async fn open_sam(
        &self,
        stream: crate::sam::raw_stream::SamIoStream,
    ) -> Result<AppGatewayConnection, AppGatewayError> {
        self.authorize(AppService::Sam)?;
        let permit = self.acquire_connection()?;
        let state = self.sam_state().await?;
        let cancellation = CancellationToken::new();
        let task_cancellation = cancellation.clone();
        let (ended_sender, ended_receiver) = oneshot::channel();
        let children = self.children.clone();
        self.children
            .spawn(move |service_cancellation| async move {
                let _permit = permit;
                let result = state
                    .drive_private_connection(
                        stream,
                        task_cancellation,
                        service_cancellation,
                        children,
                    )
                    .await;
                let ended = match result {
                    Ok(()) => AppGatewayConnectionEnd::BackendClosed,
                    Err(PrivateConnectionAdmissionError::AtCapacity) => {
                        AppGatewayConnectionEnd::BackendRejected
                    }
                };
                drop(_permit);
                let rejected = ended == AppGatewayConnectionEnd::BackendRejected;
                let _ = ended_sender.send(ended);
                if rejected {
                    Err(ChildTaskFailure::Explicit)
                } else {
                    Ok(())
                }
            })
            .map_err(|_| AppGatewayError::ResourceLimit)?;
        Ok(AppGatewayConnection {
            cancellation,
            ended: Some(ended_receiver),
        })
    }

    /// Opens a typed, bounded datagram operation stream. It is a separate
    /// manager service; the existing `sam` service remains exact-byte SAM.
    pub(crate) async fn open_datagram(
        &self,
        stream: crate::sam::raw_stream::SamIoStream,
    ) -> Result<AppGatewayConnection, AppGatewayError> {
        self.authorize(AppService::Sam)?;
        let permit = self.acquire_connection()?;
        let state = self.sam_state().await?;
        let cancellation = CancellationToken::new();
        let task_cancellation = cancellation.clone();
        let (ended_sender, ended_receiver) = oneshot::channel();
        self.children
            .spawn(move |service_cancellation| async move {
                let _permit = permit;
                state
                    .drive_private_datagram_connection(
                        stream,
                        task_cancellation,
                        service_cancellation,
                    )
                    .await;
                let _ = ended_sender.send(AppGatewayConnectionEnd::BackendClosed);
                Ok(())
            })
            .map_err(|_| AppGatewayError::ResourceLimit)?;
        Ok(AppGatewayConnection {
            cancellation,
            ended: Some(ended_receiver),
        })
    }

    /// Opens one private I2CP protocol connection over the caller-owned byte
    /// stream. No TCP listener or loopback connection is involved.
    pub(crate) async fn open_i2cp(
        &self,
        stream: crate::i2cp::I2cpIoStream,
    ) -> Result<AppGatewayConnection, AppGatewayError> {
        self.authorize(AppService::I2cp)?;
        let permit = self.acquire_connection()?;
        let state = self.i2cp_state().await?;
        let cancellation = CancellationToken::new();
        let task_cancellation = cancellation.clone();
        let (ended_sender, ended_receiver) = oneshot::channel();
        self.children
            .spawn(move |service_cancellation| async move {
                let _permit = permit;
                let result = state
                    .drive_private_connection(stream, task_cancellation, service_cancellation)
                    .await;
                let ended = match result {
                    Ok(()) => AppGatewayConnectionEnd::BackendClosed,
                    Err(I2cpPrivateConnectionError::AtCapacity) => {
                        AppGatewayConnectionEnd::BackendRejected
                    }
                };
                drop(_permit);
                let rejected = ended == AppGatewayConnectionEnd::BackendRejected;
                let _ = ended_sender.send(ended);
                if rejected {
                    Err(ChildTaskFailure::Explicit)
                } else {
                    Ok(())
                }
            })
            .map_err(|_| AppGatewayError::ResourceLimit)?;
        Ok(AppGatewayConnection {
            cancellation,
            ended: Some(ended_receiver),
        })
    }

    fn acquire_connection(&self) -> Result<OwnedSemaphorePermit, AppGatewayError> {
        self.connections
            .clone()
            .try_acquire_owned()
            .map_err(|_| AppGatewayError::ResourceLimit)
    }

    async fn sam_state(&self) -> Result<Arc<SamServiceState>, AppGatewayError> {
        let mut slot = self.sam.lock().await;
        if let Some(state) = slot.as_ref() {
            return Ok(Arc::clone(state));
        }
        let state = Arc::new(
            SamServiceState::new_with_max_supported_version(
                self.sam_config.clone(),
                self.sam_max_version,
            )
            .map_err(map_sam_error)?,
        );
        state.set_addressbook_handle(self.addressbook.clone());
        *slot = Some(Arc::clone(&state));
        Ok(state)
    }

    async fn i2cp_state(&self) -> Result<Arc<I2cpServiceState>, AppGatewayError> {
        let mut slot = self.i2cp.lock().await;
        if let Some(state) = slot.as_ref() {
            return Ok(Arc::clone(state));
        }
        let state =
            Arc::new(I2cpServiceState::new(self.i2cp_config.clone()).map_err(map_i2cp_error)?);
        *slot = Some(Arc::clone(&state));
        Ok(state)
    }

    pub(crate) async fn shutdown(&self) {
        self.cancellation
            .cancel(i2pr_core::CancellationReason::ParentScope);
        let _ = self.children.shutdown().await;
        self.sam.lock().await.take();
        self.i2cp.lock().await.take();
    }

    #[cfg(test)]
    async fn available_connection_slots(&self) -> usize {
        self.connections.available_permits()
    }

    #[cfg(test)]
    async fn has_backend_state(&self, service: AppService) -> bool {
        match service {
            AppService::Sam => self.sam.lock().await.is_some(),
            AppService::I2cp => self.i2cp.lock().await.is_some(),
            AppService::ControlScoped => false,
        }
    }
}

impl Drop for AppGatewaySession {
    fn drop(&mut self) {
        self.cancellation
            .cancel(i2pr_core::CancellationReason::ParentScope);
    }
}

/// Cancellation handle for one opened logical backend connection.
pub(crate) struct AppGatewayConnection {
    cancellation: CancellationToken,
    ended: Option<oneshot::Receiver<AppGatewayConnectionEnd>>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum AppGatewayConnectionEnd {
    /// EOF, cancellation, or protocol completion closed this backend stream.
    BackendClosed,
    /// The existing protocol owner rejected admission after gateway admission.
    BackendRejected,
    /// The supervised task exited without reporting a terminal status.
    WorkerUnavailable,
}

impl AppGatewayConnection {
    /// Waits until the backend has ended so the trusted runtime can close or
    /// reset the corresponding managed-app logical stream.
    pub(crate) async fn wait_closed(mut self) -> AppGatewayConnectionEnd {
        let Some(ended) = self.ended.take() else {
            return AppGatewayConnectionEnd::WorkerUnavailable;
        };
        ended
            .await
            .unwrap_or(AppGatewayConnectionEnd::WorkerUnavailable)
    }
}

impl AppGatewayConnection {
    /// Closes only the corresponding private protocol connection.
    pub(crate) fn close(self) {
        self.cancellation
            .cancel(i2pr_core::CancellationReason::ParentScope);
    }
}

impl Drop for AppGatewayConnection {
    fn drop(&mut self) {
        self.cancellation
            .cancel(i2pr_core::CancellationReason::ParentScope);
    }
}

#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub(crate) enum AppGatewayError {
    #[error("service capability denied")]
    PermissionDenied,
    #[error("service is unavailable")]
    UnsupportedService,
    #[error("gateway connection limit reached")]
    ResourceLimit,
    #[error("invalid gateway connection limit")]
    InvalidLimits,
    #[error("private service backend is unavailable")]
    BackendUnavailable,
}

fn map_sam_error(_: SamServiceError) -> AppGatewayError {
    AppGatewayError::BackendUnavailable
}

fn map_i2cp_error(_: I2cpServiceError) -> AppGatewayError {
    AppGatewayError::BackendUnavailable
}

#[cfg(test)]
mod tests {
    use super::*;
    use i2pr_app_proto::{
        AdministratorPrincipal, AppId, AppInstanceId, GrantedCapability, PublisherId,
        RequestedCapability,
    };
    use i2pr_runtime::ChildFailurePolicy;
    use std::net::IpAddr;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    fn principal(instance: u128) -> AppPrincipal {
        AppPrincipal {
            app_id: AppId::parse("example.client").expect("app id"),
            instance_id: AppInstanceId::new(instance).expect("instance id"),
            publisher_id: Some(PublisherId::parse("example.publisher").expect("publisher")),
        }
    }

    fn authorization(instance: u128, capabilities: &[Capability]) -> AppGatewayAuthorization {
        let administrator =
            AdministratorPrincipal::from_authenticated_session(1).expect("administrator principal");
        let grants = capabilities
            .iter()
            .map(|capability| {
                GrantedCapability::from_administrator_policy(&administrator, *capability)
                    .expect("non-reserved grant")
            })
            .collect::<Vec<_>>();
        AppGatewayAuthorization::from_trusted_composition(
            principal(instance),
            EffectiveCapabilities::from_grants(&grants).expect("effective grants"),
        )
    }

    fn session(instance: u128, capabilities: &[Capability]) -> AppGatewaySession {
        let cancellation = CancellationToken::new();
        let children = ChildScope::for_test(&cancellation, ChildFailurePolicy::CollectResult);
        AppGatewaySession::new(
            authorization(instance, capabilities),
            AppGatewayLimits::new(MAX_GATEWAY_CONNECTIONS).expect("valid limits"),
            AppGatewayComposition {
                sam: SamConfig {
                    enabled: true,
                    bind_address: IpAddr::V4(std::net::Ipv4Addr::LOCALHOST),
                    port: 7656,
                    udp_port: 0,
                    limits: i2pr_api::sam::limits::SamLimits::loopback_test_profile(),
                },
                sam_max_version: i2pr_api::sam::version::SamVersion::const_new(3, 3),
                i2cp: I2cpConfig::loopback_test_profile(8, 16, 64 * 1024, 64),
                addressbook: SharedAddressBook::new(),
                children,
                cancellation,
            },
        )
    }

    async fn read_line(stream: &mut tokio::io::DuplexStream) -> String {
        let mut line = Vec::new();
        loop {
            let mut byte = [0_u8; 1];
            stream.read_exact(&mut byte).await.expect("reply byte");
            line.push(byte[0]);
            if byte[0] == b'\n' {
                return String::from_utf8(line).expect("SAM reply UTF-8");
            }
        }
    }

    #[tokio::test]
    async fn denied_and_reserved_opens_have_no_backend_side_effects() {
        let session = session(1, &[Capability::ControlScoped]);
        assert_eq!(
            session.authorize(AppService::Sam),
            Err(AppGatewayError::PermissionDenied)
        );
        assert_eq!(
            session.authorize(AppService::I2cp),
            Err(AppGatewayError::PermissionDenied)
        );
        assert_eq!(
            session.authorize(AppService::ControlScoped),
            Err(AppGatewayError::UnsupportedService)
        );
        let requested = RequestedCapability {
            capability: Capability::Sam,
        };
        assert_eq!(requested.capability, Capability::Sam);
        assert!(
            !session
                .authorization
                .effective_capabilities
                .as_slice()
                .contains(&Capability::Sam)
        );
        let administrator =
            AdministratorPrincipal::from_authenticated_session(2).expect("administrator principal");
        assert!(
            GrantedCapability::from_administrator_policy(&administrator, Capability::BrokeredTcp)
                .is_err()
        );
        let (_client, router) = tokio::io::duplex(128);
        assert!(matches!(
            session.open_sam(Box::new(router)).await,
            Err(AppGatewayError::PermissionDenied)
        ));
        let (_client, datagram_router) = tokio::io::duplex(128);
        assert!(matches!(
            session.open_datagram(Box::new(datagram_router)).await,
            Err(AppGatewayError::PermissionDenied)
        ));
        assert!(!session.has_backend_state(AppService::Sam).await);
        assert!(!session.has_backend_state(AppService::I2cp).await);
        assert_eq!(session.available_connection_slots().await, 8);
        session.shutdown().await;
    }

    #[tokio::test]
    async fn private_sam_sessions_are_instance_scoped_and_forward_stays_denied() {
        let first = session(1, &[Capability::Sam]);
        let second = session(2, &[Capability::Sam]);
        let first_state = first.sam_state().await.expect("first private SAM state");
        let second_state = second.sam_state().await.expect("second private SAM state");
        assert!(!Arc::ptr_eq(&first_state, &second_state));
        assert!(!first_state.config().enabled);
        assert!(!second_state.config().enabled);

        let (mut client_a, router_a) = tokio::io::duplex(4096);
        let (mut client_b, router_b) = tokio::io::duplex(4096);
        let connection_a = first
            .open_sam(Box::new(router_a))
            .await
            .expect("open SAM A");
        let connection_b = second
            .open_sam(Box::new(router_b))
            .await
            .expect("open SAM B");
        for client in [&mut client_a, &mut client_b] {
            client
                .write_all(b"HELLO VERSION MIN=3.1 MAX=3.1\n")
                .await
                .expect("HELLO");
            assert!(read_line(client).await.contains("HELLO REPLY RESULT=OK"));
        }
        client_a
            .write_all(b"SESSION CREATE STYLE=STREAM ID=shared DESTINATION=TRANSIENT\n")
            .await
            .expect("create first session");
        assert!(
            read_line(&mut client_a)
                .await
                .starts_with("SESSION STATUS RESULT=OK")
        );
        client_b
            .write_all(b"STREAM ACCEPT ID=shared\n")
            .await
            .expect("cross-principal attach attempt");
        let denied_attach = read_line(&mut client_b).await;
        assert!(
            denied_attach.contains("RESULT=I2P_ERROR")
                || denied_attach.contains("RESULT=INVALID_ID"),
            "unexpected reply: {denied_attach}"
        );
        assert_eq!(
            connection_b.wait_closed().await,
            AppGatewayConnectionEnd::BackendClosed
        );
        drop(client_b);

        let (mut client_b, router_b) = tokio::io::duplex(4096);
        let connection_b = second
            .open_sam(Box::new(router_b))
            .await
            .expect("reopen SAM B");
        client_b
            .write_all(b"HELLO VERSION MIN=3.1 MAX=3.1\n")
            .await
            .expect("second HELLO");
        assert!(
            read_line(&mut client_b)
                .await
                .contains("HELLO REPLY RESULT=OK")
        );
        client_b
            .write_all(b"SESSION CREATE STYLE=STREAM ID=shared DESTINATION=TRANSIENT\n")
            .await
            .expect("reuse local id");
        assert!(
            read_line(&mut client_b)
                .await
                .starts_with("SESSION STATUS RESULT=OK")
        );
        client_a
            .write_all(b"STREAM FORWARD ID=private PORT=1 HOST=127.0.0.1\n")
            .await
            .expect("FORWARD");
        assert!(read_line(&mut client_a).await.contains("RESULT=I2P_ERROR"));
        drop(client_a);
        assert_eq!(
            connection_a.wait_closed().await,
            AppGatewayConnectionEnd::BackendClosed
        );
        drop(client_b);
        assert_eq!(
            connection_b.wait_closed().await,
            AppGatewayConnectionEnd::BackendClosed
        );
        assert_eq!(first.available_connection_slots().await, 8);
        first.shutdown().await;
        second.shutdown().await;
    }

    #[tokio::test]
    async fn private_sam_primary_subsessions_are_scoped_to_the_app_instance() {
        use tokio::io::AsyncWriteExt;

        let first = session(41, &[Capability::Sam]);
        let second = session(42, &[Capability::Sam]);
        let first_state = first.sam_state().await.expect("private SAM state");

        let (mut primary_client, primary_router) = tokio::io::duplex(4096);
        let primary_connection = first
            .open_sam(Box::new(primary_router))
            .await
            .expect("open primary control stream");
        primary_client
            .write_all(b"HELLO VERSION MIN=3.1 MAX=3.3\n")
            .await
            .expect("HELLO primary");
        assert!(read_line(&mut primary_client).await.contains("VERSION=3.3"));
        primary_client
            .write_all(b"SESSION CREATE STYLE=PRIMARY ID=shared DESTINATION=TRANSIENT\n")
            .await
            .expect("create primary");
        assert!(
            read_line(&mut primary_client)
                .await
                .starts_with("SESSION STATUS RESULT=OK DESTINATION=")
        );
        primary_client
            .write_all(b"SESSION ADD STYLE=STREAM ID=mail FROM_PORT=25 TO_PORT=110\n")
            .await
            .expect("add child");
        assert_eq!(
            read_line(&mut primary_client).await,
            "SESSION STATUS RESULT=OK ID=\"mail\"\n"
        );

        // A second private SAM connection from the same app instance may use
        // the child ID. Its invalid destination is reached only after the
        // child lookup succeeds.
        let (mut same_app_client, same_app_router) = tokio::io::duplex(4096);
        let same_app_connection = first
            .open_sam(Box::new(same_app_router))
            .await
            .expect("open same-app stream socket");
        same_app_client
            .write_all(b"HELLO VERSION MIN=3.3 MAX=3.3\n")
            .await
            .expect("HELLO same app");
        assert!(
            read_line(&mut same_app_client)
                .await
                .contains("VERSION=3.3")
        );
        same_app_client
            .write_all(b"STREAM CONNECT ID=mail DESTINATION=invalid\n")
            .await
            .expect("connect using child ID");
        assert!(
            read_line(&mut same_app_client)
                .await
                .contains("RESULT=INVALID_KEY")
        );

        // A different app instance reusing the same textual session ID must
        // fail at session lookup before destination parsing.
        let (mut other_app_client, other_app_router) = tokio::io::duplex(4096);
        let other_app_connection = second
            .open_sam(Box::new(other_app_router))
            .await
            .expect("open other-app stream socket");
        other_app_client
            .write_all(b"HELLO VERSION MIN=3.3 MAX=3.3\n")
            .await
            .expect("HELLO other app");
        assert!(
            read_line(&mut other_app_client)
                .await
                .contains("VERSION=3.3")
        );
        other_app_client
            .write_all(b"STREAM CONNECT ID=mail DESTINATION=invalid\n")
            .await
            .expect("cross-app connect");
        assert!(
            read_line(&mut other_app_client)
                .await
                .contains("RESULT=INVALID_ID")
        );

        drop(primary_client);
        assert_eq!(
            primary_connection.wait_closed().await,
            AppGatewayConnectionEnd::BackendClosed
        );
        assert_eq!(first_state.session_registry().session_count(), 0);
        drop((same_app_client, other_app_client));
        same_app_connection.wait_closed().await;
        other_app_connection.wait_closed().await;
        first.shutdown().await;
        second.shutdown().await;
    }

    #[tokio::test]
    async fn private_sam_stream_children_route_nonzero_and_maximum_ports() {
        let app = session(51, &[Capability::Sam]);
        let (mut peer, peer_router) = tokio::io::duplex(8192);
        let _peer_connection = app
            .open_sam(Box::new(peer_router))
            .await
            .expect("open peer private SAM stream");
        peer.write_all(b"HELLO VERSION MIN=3.3 MAX=3.3\n")
            .await
            .expect("peer HELLO");
        assert!(read_line(&mut peer).await.contains("VERSION=3.3"));
        peer.write_all(b"SESSION CREATE STYLE=PRIMARY ID=peer DESTINATION=TRANSIENT\n")
            .await
            .expect("create peer primary");
        assert!(
            read_line(&mut peer)
                .await
                .starts_with("SESSION STATUS RESULT=OK")
        );
        for (id, port) in [("pop", 110), ("max-listener", u16::MAX)] {
            let command = format!("SESSION ADD STYLE=STREAM ID={id} FROM_PORT={port}\n");
            peer.write_all(command.as_bytes())
                .await
                .expect("add peer STREAM child");
            assert!(read_line(&mut peer).await.contains(&format!("ID=\"{id}\"")));
        }
        peer.write_all(b"NAMING LOOKUP NAME=ME\n")
            .await
            .expect("lookup peer Destination");
        let naming = read_line(&mut peer).await;
        let peer_public = naming
            .split_whitespace()
            .find_map(|field| field.strip_prefix("VALUE="))
            .expect("peer public Destination")
            .trim_matches(['"', '\n', '\r'])
            .to_owned();

        let (mut primary, primary_router) = tokio::io::duplex(8192);
        let _primary_connection = app
            .open_sam(Box::new(primary_router))
            .await
            .expect("open client private SAM stream");
        primary
            .write_all(b"HELLO VERSION MIN=3.3 MAX=3.3\n")
            .await
            .expect("client HELLO");
        assert!(read_line(&mut primary).await.contains("VERSION=3.3"));
        primary
            .write_all(b"SESSION CREATE STYLE=PRIMARY ID=client DESTINATION=TRANSIENT\n")
            .await
            .expect("create client primary");
        assert!(
            read_line(&mut primary)
                .await
                .starts_with("SESSION STATUS RESULT=OK")
        );
        for (id, from_port) in [("mail", 25), ("max-client", u16::MAX)] {
            let command = format!("SESSION ADD STYLE=STREAM ID={id} FROM_PORT={from_port}\n");
            primary
                .write_all(command.as_bytes())
                .await
                .expect("add client STREAM child");
            assert!(
                read_line(&mut primary)
                    .await
                    .contains(&format!("ID=\"{id}\""))
            );
        }

        let (mut omitted, omitted_router) = tokio::io::duplex(8192);
        let _omitted_connection = app
            .open_sam(Box::new(omitted_router))
            .await
            .expect("open omitted-port private SAM stream");
        omitted
            .write_all(b"HELLO VERSION MIN=3.3 MAX=3.3\n")
            .await
            .expect("omitted HELLO");
        assert!(read_line(&mut omitted).await.contains("VERSION=3.3"));
        let command = format!("STREAM CONNECT ID=mail DESTINATION={peer_public}\n");
        omitted
            .write_all(command.as_bytes())
            .await
            .expect("connect without TO_PORT");
        let result =
            tokio::time::timeout(std::time::Duration::from_secs(22), read_line(&mut omitted))
                .await
                .expect("bounded omitted-port result");
        assert!(
            result.contains("RESULT=TIMEOUT"),
            "unexpected reply {result:?}"
        );

        async fn exchange(
            app: &AppGatewaySession,
            child_id: &str,
            peer_id: &str,
            peer_public: &str,
            port: u16,
        ) {
            let (mut accept, accept_router) = tokio::io::duplex(8192);
            let _accept_connection = app
                .open_sam(Box::new(accept_router))
                .await
                .expect("open private STREAM ACCEPT");
            accept
                .write_all(b"HELLO VERSION MIN=3.3 MAX=3.3\n")
                .await
                .expect("accept HELLO");
            assert!(read_line(&mut accept).await.contains("VERSION=3.3"));
            accept
                .write_all(format!("STREAM ACCEPT ID={peer_id}\n").as_bytes())
                .await
                .expect("STREAM ACCEPT");

            let (mut connect, connect_router) = tokio::io::duplex(8192);
            let _connect_connection = app
                .open_sam(Box::new(connect_router))
                .await
                .expect("open private STREAM CONNECT");
            connect
                .write_all(b"HELLO VERSION MIN=3.3 MAX=3.3\n")
                .await
                .expect("connect HELLO");
            assert!(read_line(&mut connect).await.contains("VERSION=3.3"));
            let command =
                format!("STREAM CONNECT ID={child_id} DESTINATION={peer_public} TO_PORT={port}\n");
            connect
                .write_all(command.as_bytes())
                .await
                .expect("STREAM CONNECT");
            assert!(
                read_line(&mut connect)
                    .await
                    .starts_with("STREAM STATUS RESULT=OK")
            );
            assert!(
                read_line(&mut accept)
                    .await
                    .starts_with("STREAM STATUS RESULT=OK")
            );
            assert!(read_line(&mut accept).await.starts_with("DESTINATION="));
            let payload = format!("private-port-{port}").into_bytes();
            connect.write_all(&payload).await.expect("send payload");
            let mut received = vec![0_u8; payload.len()];
            accept
                .read_exact(&mut received)
                .await
                .expect("receive payload");
            assert_eq!(received, payload);
        }

        exchange(&app, "mail", "pop", &peer_public, 110).await;
        exchange(&app, "max-client", "max-listener", &peer_public, u16::MAX).await;
        let state = app.sam_state().await.expect("app SAM state");
        assert_eq!(state.session_registry().session_count(), 6);
        app.shutdown().await;
        assert_eq!(state.session_registry().session_count(), 0);
    }

    #[tokio::test]
    async fn private_i2cp_get_date_runs_in_isolated_instance_contexts() {
        let first = session(1, &[Capability::I2cp]);
        let second = session(2, &[Capability::I2cp]);
        let first_state = first.i2cp_state().await.expect("first private I2CP state");
        let second_state = second
            .i2cp_state()
            .await
            .expect("second private I2CP state");
        assert!(!Arc::ptr_eq(&first_state, &second_state));
        assert!(!first_state.config().enabled);
        assert!(!second_state.config().enabled);
        let (mut client_a, router_a) = tokio::io::duplex(4096);
        let (mut client_b, router_b) = tokio::io::duplex(4096);
        let connection_a = first
            .open_i2cp(Box::new(router_a))
            .await
            .expect("open I2CP A");
        let connection_b = second
            .open_i2cp(Box::new(router_b))
            .await
            .expect("open I2CP B");
        for client in [&mut client_a, &mut client_b] {
            client
                .write_all(&[i2pr_api::i2cp::PROTOCOL_BYTE])
                .await
                .expect("protocol byte");
            let request = i2pr_api::i2cp::Message::GetDate(i2pr_api::i2cp::GetDate {
                version: "0.9.67".to_owned(),
                auth: None,
            });
            let body = request.encode_body().expect("GetDate body");
            let frame = i2pr_api::i2cp::encode_frame(request.message_type() as u8, &body)
                .expect("GetDate frame");
            client.write_all(&frame).await.expect("GetDate");
            let mut header = [0_u8; 5];
            client
                .read_exact(&mut header)
                .await
                .expect("SetDate header");
            let length = u32::from_be_bytes(header[..4].try_into().expect("length")) as usize;
            let mut response = vec![0; length];
            client
                .read_exact(&mut response)
                .await
                .expect("SetDate body");
            assert_eq!(header[4], i2pr_api::i2cp::MessageType::SetDate as u8);
        }
        assert_eq!(first_state.active_connection_ids_for_test(), vec![1]);
        assert_eq!(second_state.active_connection_ids_for_test(), vec![1]);
        drop((client_a, client_b));
        assert_eq!(
            connection_a.wait_closed().await,
            AppGatewayConnectionEnd::BackendClosed
        );
        assert_eq!(
            connection_b.wait_closed().await,
            AppGatewayConnectionEnd::BackendClosed
        );
        first.shutdown().await;
        second.shutdown().await;
    }

    #[tokio::test]
    async fn one_backend_eof_does_not_close_a_sibling_connection() {
        let session = session(3, &[Capability::Sam]);
        let (mut client_a, router_a) = tokio::io::duplex(4096);
        let (mut client_b, router_b) = tokio::io::duplex(4096);
        let connection_a = session
            .open_sam(Box::new(router_a))
            .await
            .expect("open SAM sibling A");
        let connection_b = session
            .open_sam(Box::new(router_b))
            .await
            .expect("open SAM sibling B");
        for client in [&mut client_a, &mut client_b] {
            client
                .write_all(b"HELLO VERSION MIN=3.1 MAX=3.1\n")
                .await
                .expect("HELLO");
            assert!(read_line(client).await.contains("HELLO REPLY RESULT=OK"));
        }

        drop(client_a);
        assert_eq!(
            connection_a.wait_closed().await,
            AppGatewayConnectionEnd::BackendClosed
        );
        client_b
            .write_all(b"SESSION CREATE STYLE=STREAM ID=sibling DESTINATION=TRANSIENT\n")
            .await
            .expect("sibling remains usable");
        assert!(
            read_line(&mut client_b)
                .await
                .starts_with("SESSION STATUS RESULT=OK")
        );
        drop(client_b);
        assert_eq!(
            connection_b.wait_closed().await,
            AppGatewayConnectionEnd::BackendClosed
        );
        session.shutdown().await;
    }

    #[tokio::test]
    async fn connection_admission_is_bounded_and_cleanup_releases_capacity() {
        let cancellation = CancellationToken::new();
        let children = ChildScope::for_test(&cancellation, ChildFailurePolicy::CollectResult);
        let session = AppGatewaySession::new(
            authorization(1, &[Capability::Sam]),
            AppGatewayLimits::new(1).expect("single slot"),
            AppGatewayComposition {
                sam: SamConfig {
                    enabled: false,
                    bind_address: IpAddr::V4(std::net::Ipv4Addr::LOCALHOST),
                    port: 0,
                    udp_port: 0,
                    limits: i2pr_api::sam::limits::SamLimits::loopback_test_profile(),
                },
                sam_max_version: i2pr_api::sam::version::SamVersion::const_new(3, 3),
                i2cp: I2cpConfig::loopback_test_profile(8, 16, 64 * 1024, 64),
                addressbook: SharedAddressBook::new(),
                children,
                cancellation,
            },
        );
        let (_client, router) = tokio::io::duplex(128);
        let _open = session
            .open_sam(Box::new(router))
            .await
            .expect("first open");
        assert_eq!(session.available_connection_slots().await, 0);
        let (_client, extra) = tokio::io::duplex(128);
        assert!(matches!(
            session.open_sam(Box::new(extra)).await,
            Err(AppGatewayError::ResourceLimit)
        ));
        assert!(session.has_backend_state(AppService::Sam).await);
        session.shutdown().await;
        assert_eq!(session.available_connection_slots().await, 1);
    }
}
