use std::time::Duration;

use i2pr_testkit::{
    FaultScript, LinkId, ManualClock, NetworkScheduler, ReproducibilitySeed, SchedulerConfig,
    StreamConfig,
};
use i2pr_transport_ntcp2::handshake::{
    ConfirmedPayload, SessionConfirmed, SessionCreated, SessionRequest,
};

fn deliver_one_byte_at_a_time(
    scheduler: &NetworkScheduler,
    sender: &i2pr_testkit::StreamEndpoint,
    receiver: &i2pr_testkit::StreamEndpoint,
    payload: &[u8],
) -> Vec<u8> {
    let mut received = Vec::with_capacity(payload.len());
    let mut offset = 0;
    while offset < payload.len() {
        assert_eq!(
            sender.try_write(&payload[offset..]).expect("partial write"),
            1
        );
        offset += 1;
        scheduler.advance(Duration::ZERO).expect("delivery");
        let mut byte = [0_u8; 1];
        assert_eq!(receiver.try_read(&mut byte).expect("partial read"), Some(1));
        received.push(byte[0]);
    }
    received
}

#[test]
fn handshake_messages_survive_one_byte_testkit_segments() {
    let scheduler =
        NetworkScheduler::new(ManualClock::new(), SchedulerConfig::default()).expect("scheduler");
    let link = scheduler
        .stream_link(
            LinkId::new(33).expect("link id"),
            StreamConfig::new(65_535, 1).expect("stream config"),
            FaultScript::empty(ReproducibilitySeed::from_u128(0x033)),
        )
        .expect("stream link");
    let sender = link.left();
    let receiver = link.right();

    let request = SessionRequest::new([1; 32], vec![2; 32], vec![3; 5])
        .expect("request")
        .encode();
    let created = SessionCreated::new([4; 32], vec![5; 32], vec![6; 7])
        .expect("created")
        .encode();
    let payload = ConfirmedPayload::new(vec![9; 12], None, Some(vec![7; 2]))
        .expect("payload")
        .encode();
    let confirmed = SessionConfirmed::new(vec![8; 48], {
        let mut frame = payload;
        frame.extend_from_slice(&[0; 16]);
        frame
    })
    .expect("confirmed")
    .encode();

    let request_received = deliver_one_byte_at_a_time(&scheduler, &sender, &receiver, &request);
    assert_eq!(
        SessionRequest::decode(&request_received, 65_535).expect("request decode"),
        SessionRequest::decode(&request, 65_535).expect("request source decode")
    );
    let created_received = deliver_one_byte_at_a_time(&scheduler, &sender, &receiver, &created);
    assert_eq!(
        SessionCreated::decode(&created_received, 65_535).expect("created decode"),
        SessionCreated::decode(&created, 65_535).expect("created source decode")
    );
    let confirmed_received = deliver_one_byte_at_a_time(&scheduler, &sender, &receiver, &confirmed);
    let confirmed_decoded =
        SessionConfirmed::decode(&confirmed_received, confirmed.len() - 48, 65_535)
            .expect("confirmed decode");
    assert_eq!(confirmed_decoded.encode(), confirmed);
}
