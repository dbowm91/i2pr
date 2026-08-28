use i2pr_api::sam::limits::SamLimits;
use i2pr_api::sam::parser::parse_line;
use i2pr_api::sam::streams::{InboundMode, SamStreamRegistry, SamStreamRegistryError};
use i2pr_client::DestinationId;

fn destination(seed: u8) -> DestinationId {
    let mut bytes = [0_u8; 32];
    bytes[0] = seed;
    DestinationId::from_hash(i2pr_proto::Hash::from_bytes(bytes))
}

#[test]
fn forward_parser_requires_port_and_rejects_ssl_explicitly() {
    let missing = parse_line("STREAM FORWARD ID=alpha HOST=127.0.0.1").unwrap();
    assert!(matches!(
        missing,
        i2pr_api::CommandOutcome::Recognised(command)
            if matches!(
                i2pr_api::parse_stream_forward(&command),
                Err(i2pr_api::StreamForwardError::MissingPort)
            )
    ));
    let ssl = parse_line("STREAM FORWARD ID=alpha PORT=80 SSL=true").unwrap();
    assert!(matches!(
        ssl,
        i2pr_api::CommandOutcome::Unsupported(unsupported)
            if unsupported.reason == i2pr_api::sam::command::UnsupportedReason::StreamForwardSsl
    ));
}

#[test]
fn inbound_accept_and_forward_share_one_atomic_mode() {
    let registry = SamStreamRegistry::new(SamLimits::defaults());
    let session = i2pr_api::SamSessionId::new("alpha").unwrap();
    registry.register_session(session.clone()).unwrap();

    let waiter = registry
        .register_inbound_waiter(&session, destination(1))
        .unwrap();
    assert_eq!(
        registry.register_forward(&session, 10),
        Err(SamStreamRegistryError::AcceptAlreadyPending)
    );
    registry
        .release_attachment(&session, waiter.stream_id)
        .unwrap();
    registry.register_forward(&session, 10).unwrap();
    assert_eq!(
        registry.register_inbound_waiter(&session, destination(1)),
        Err(SamStreamRegistryError::ForwardAlreadyActive)
    );
    assert_eq!(
        registry.inbound_mode(&session).unwrap(),
        InboundMode::Forwarding { owner: 10 }
    );
    assert!(registry.release_forward(&session, 10).unwrap());
    assert_eq!(registry.inbound_mode(&session).unwrap(), InboundMode::Idle);
}

#[test]
fn naming_never_resolves_clearnet_names() {
    let command = parse_line("NAMING LOOKUP NAME=service.i2p").unwrap();
    let request = i2pr_api::parse_naming_lookup(command.command().unwrap()).unwrap();
    assert_eq!(
        i2pr_api::resolve_public_destination(&request.name),
        Err(i2pr_api::NamingLookupError::KeyNotFound)
    );
}

#[test]
fn unsupported_m7_command_families_are_typed() {
    for line in [
        "SESSION CREATE STYLE=RAW ID=alpha",
        "SESSION CREATE STYLE=DATAGRAM2 ID=alpha",
        "SESSION CREATE STYLE=DATAGRAM3 ID=alpha",
        "SESSION CREATE STYLE=PRIMARY ID=alpha",
        "SESSION ADD ID=alpha",
        "SESSION REMOVE ID=alpha",
        "STREAM CONNECT ID=alpha DESTINATION=foo TO_PORT=80",
        "AUTH USER=alice",
        "DATAGRAM SEND ID=alpha",
        "RAW SEND ID=alpha",
    ] {
        assert!(
            matches!(
                parse_line(line).unwrap(),
                i2pr_api::CommandOutcome::Unsupported(_)
            ),
            "{line}"
        );
    }
}
