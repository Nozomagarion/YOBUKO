// Copyright (C) 2026 The Suhina contributors
// SPDX-License-Identifier: GPL-3.0-or-later

use super::*;

#[test]
fn the_estimate_keeps_the_fastest_round_trip_and_forgets_old_ones() {
    let mut clock = ClockEstimate::default();
    assert_eq!(clock.offset(), None);
    // Host clock is 5000 ms ahead. A slow exchange whose reply waited on the way back skews the middle.
    clock.add(1000, 1400, 6050);
    assert_eq!(clock.offset(), Some(6050 - 1200));
    clock.add(2000, 2010, 7005);
    assert_eq!(clock.offset(), Some(5000), "the faster round trip wins");
    clock.add(3000, 3200, 8180);
    assert_eq!(clock.offset(), Some(5000));
    // Useless samples are ignored: a reply before its request, a reply after the limit.
    clock.add(4000, 3990, 9000);
    clock.add(4000, 4000 + MAX_RTT_MS + 1, 9000);
    assert_eq!(clock.samples.len(), 3);
    for i in 0..SAMPLES as u64 {
        clock.add(10_000 + i * 100, 10_000 + i * 100 + 40, 15_020 + i * 100 + 3);
    }
    assert_eq!(clock.samples.len(), SAMPLES);
    assert_eq!(clock.offset(), Some(5003), "the fast sample from before the window is gone");
}

#[test]
fn a_negative_offset_and_extreme_stamps_do_not_overflow() {
    let mut clock = ClockEstimate::default();
    clock.add(90_000, 90_020, 10);
    assert_eq!(clock.offset(), Some(10 - 90_010));
    clock.add(u64::MAX - 10, u64::MAX, u64::MAX);
    assert!(clock.offset().is_some());
    assert_eq!(starts_in_ms(u64::MAX, i64::MIN), i64::MAX);
    assert_eq!(starts_in_ms(1_300, 1_000), 300);
    assert_eq!(starts_in_ms(1_000, 1_250), -250);
}

#[test]
fn a_late_start_jumps_ahead_by_the_time_already_played() {
    assert_eq!(beat_after(4.0, 0, 120.0), 4.0);
    assert_eq!(beat_after(4.0, -200, 120.0), 4.0, "an early call stays at the start");
    assert!((beat_after(4.0, 500, 120.0) - 5.0).abs() < 1e-9);
    assert!((beat_after(0.0, 1000, 90.0) - 1.5).abs() < 1e-9);
    assert_eq!(beat_after(MAX_BEAT - 1.0, i64::MAX, 999.0), MAX_BEAT);
    assert!((beat_after(0.0, 1000, f64::NAN) - 2.0).abs() < 1e-9, "a bad tempo falls back to 120");
}

#[test]
fn the_host_orders_link_play_seek_and_stop() {
    let mut linked = Linked::new();
    assert_eq!(linked.view(), None);
    assert!(linked.apply_at(TransportRequest::Play { beat: 0.0 }, 10).is_err(), "unlinked play is refused");
    linked.apply_at(TransportRequest::Link { linked: true }, 10).unwrap();
    assert_eq!(linked.view(), Some(SharedTransport { seq: 1, playing: false, beat: 0.0, start_ms: 10 }));
    linked.apply_at(TransportRequest::Link { linked: true }, 20).unwrap();
    assert_eq!(linked.seq(), 1, "linking twice changes nothing");
    linked.apply_at(TransportRequest::Play { beat: 8.0 }, 1000).unwrap();
    assert_eq!(linked.view(), Some(SharedTransport { seq: 2, playing: true, beat: 8.0, start_ms: 1000 + LEAD_MS }));
    linked.apply_at(TransportRequest::Seek { beat: 16.0 }, 2000).unwrap();
    assert_eq!(linked.view(), Some(SharedTransport { seq: 3, playing: true, beat: 16.0, start_ms: 2000 + LEAD_MS }));
    linked.apply_at(TransportRequest::Stop {}, 3000).unwrap();
    assert_eq!(linked.view(), Some(SharedTransport { seq: 4, playing: false, beat: 16.0, start_ms: 3000 }));
    linked.apply_at(TransportRequest::Seek { beat: 2.0 }, 3500).unwrap();
    assert_eq!(linked.view(), Some(SharedTransport { seq: 5, playing: false, beat: 2.0, start_ms: 3500 }));
    for beat in [f64::NAN, f64::INFINITY, -1.0, MAX_BEAT + 1.0] {
        assert!(linked.apply_at(TransportRequest::Play { beat }, 4000).is_err());
        assert!(linked.apply_at(TransportRequest::Seek { beat }, 4000).is_err());
    }
    assert_eq!(linked.seq(), 5, "refused requests change nothing");
    linked.apply_at(TransportRequest::Link { linked: false }, 5000).unwrap();
    assert_eq!(linked.view(), None);
    assert!(linked.apply_at(TransportRequest::Stop {}, 5000).is_err());
    linked.apply_at(TransportRequest::Link { linked: true }, 6000).unwrap();
    assert_eq!(linked.seq(), 7, "the counter keeps rising across relinks");
}

#[test]
fn requests_and_shared_transports_parse_strictly() {
    for (text, request) in [
        (r#"{"action":"link","linked":true}"#, TransportRequest::Link { linked: true }),
        (r#"{"action":"play","beat":4.5}"#, TransportRequest::Play { beat: 4.5 }),
        (r#"{"action":"stop"}"#, TransportRequest::Stop {}),
        (r#"{"action":"seek","beat":0}"#, TransportRequest::Seek { beat: 0.0 }),
    ] {
        assert_eq!(serde_json::from_str::<TransportRequest>(text).unwrap(), request);
    }
    for text in [
        r#"{"action":"rewind"}"#,
        r#"{"action":"stop","beat":1}"#,
        r#"{"action":"play"}"#,
        r#"{"action":"play","beat":"1"}"#,
    ] {
        assert!(serde_json::from_str::<TransportRequest>(text).is_err(), "{text}");
    }
    assert!(
        serde_json::from_str::<SharedTransport>(r#"{"seq":1,"playing":true,"beat":1,"start_ms":2,"tempo":1}"#).is_err()
    );
    assert!(SharedTransport { seq: 1, playing: true, beat: -1.0, start_ms: 0 }.validate().is_err());
}
