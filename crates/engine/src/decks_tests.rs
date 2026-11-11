// Copyright (C) 2026 The Suhina contributors
// SPDX-License-Identifier: GPL-3.0-or-later

#![allow(clippy::unwrap_used)]
use super::*;
use std::sync::Arc;
fn signal(value: f32, rate: u32) -> Arc<AudioData> {
    Arc::new(AudioData { sample_rate: rate, channels: 2, samples: vec![value; rate as usize * 4] })
}
fn setup() -> (Renderer, Controller) {
    let mut r = Renderer::stopped(
        &suhina_model::Project::default(),
        &AudioPool::new(),
        8000,
        suhina_device_registry::factory(),
    )
    .unwrap();
    let c = r.controller();
    (r, c)
}
fn block(r: &mut Renderer) -> Vec<f32> {
    let mut b = vec![0.0; 2048];
    r.process(&mut b);
    b
}
#[test]
fn decks_mix_independent_sample_rates_at_crossfader_endpoints_and_midpoint() {
    let (mut r, mut c) = setup();
    c.deck_play(0, signal(0.2, 8000), 0.0).unwrap();
    c.deck_play(1, signal(0.4, 16000), 0.5).unwrap();
    for (x, expected) in [(0.0, 0.2), (0.5, 0.3), (1.0, 0.4)] {
        c.deck_mix(x, [1.0; 2], [false; 2]).unwrap();
        let b = block(&mut r);
        assert!((b[1600] - expected * 0.501_187_2).abs() < 1e-5);
    }
    let a = c.deck_monitor(0).unwrap().seconds();
    let b = c.deck_monitor(1).unwrap().seconds();
    assert!((b - a - 0.5).abs() < 0.001);
    c.deck_stop(0).unwrap();
    let out = block(&mut r);
    assert!((out[1600] - 0.4 * 0.501_187_2).abs() < 1e-5);
    assert!(c.deck_monitor(1).unwrap().playing());
    assert!(!c.deck_monitor(0).unwrap().playing());
}
#[test]
fn deck_pause_resume_seek_end_and_replay_use_the_same_source_clock() {
    let (mut r, mut c) = setup();
    let data = signal(0.3, 8000);
    c.deck_play(0, data.clone(), 0.0).unwrap();
    block(&mut r);
    let at = c.deck_monitor(0).unwrap().seconds();
    c.deck_pause(0).unwrap();
    block(&mut r);
    assert_eq!(c.deck_monitor(0).unwrap().seconds(), at);
    assert!(!c.deck_monitor(0).unwrap().playing());
    c.deck_play(0, data.clone(), 0.0).unwrap();
    block(&mut r);
    assert!((c.deck_monitor(0).unwrap().seconds() - at - 0.128).abs() < 1e-6);
    c.deck_pause(0).unwrap();
    c.deck_seek(0, 1.0).unwrap();
    block(&mut r);
    assert_eq!(c.deck_monitor(0).unwrap().seconds(), 1.0);
    c.deck_play(0, data.clone(), 0.0).unwrap();
    block(&mut r);
    assert!((c.deck_monitor(0).unwrap().seconds() - 1.128).abs() < 1e-6);
    c.deck_seek(0, 1.99).unwrap();
    block(&mut r);
    assert!(!c.deck_monitor(0).unwrap().playing());
    c.deck_play(0, data, 0.0).unwrap();
    block(&mut r);
    assert!(c.deck_monitor(0).unwrap().playing());
}
#[test]
fn deck_headphones_are_before_crossfader_and_do_not_leak_into_main() {
    let (mut r, mut c) = setup();
    c.deck_play(1, signal(0.4, 8000), 0.0).unwrap();
    c.deck_mix(0.0, [1.0; 2], [false, true]).unwrap();
    c.send(LaunchCommand::SetCue { gain: Some(0.5), separate: Some(true) }).unwrap();
    let (mut main, mut cue) = (vec![0.0; 2048], vec![0.0; 2048]);
    r.process_with_cue(&mut main, &mut cue);
    assert!(main.iter().all(|s| *s == 0.0));
    assert!((cue[1600] - 0.4 * 0.501_187_2 * 0.5).abs() < 1e-5);
    c.send(LaunchCommand::SetCue { gain: None, separate: Some(false) }).unwrap();
    r.process_with_cue(&mut main, &mut cue);
    assert!(main.iter().chain(cue.iter()).all(|s| *s == 0.0));
}
#[test]
fn deck_queue_pressure_is_bounded_and_retired_audio_is_freed_by_the_controller() {
    let (mut r, mut c) = setup();
    let data = signal(0.2, 8000);
    let weak = Arc::downgrade(&data);
    c.deck_play(0, data, 0.0).unwrap();
    block(&mut r);
    c.deck_play(0, signal(0.3, 8000), 0.0).unwrap();
    block(&mut r);
    assert!(weak.upgrade().is_some(), "the audio callback must retire rather than destroy the old sample");
    c.deck_pause(0).unwrap();
    assert!(weak.upgrade().is_none());
    block(&mut r);
    for _ in 0..16 {
        c.deck_seek(0, 0.0).unwrap();
    }
    assert!(c.deck_stop(0).is_err());
    block(&mut r);
    c.deck_stop(0).unwrap();
    assert!(block(&mut r).iter().all(|s| *s == 0.0));
    assert!(c.deck_mix(f32::NAN, [1.0; 2], [false; 2]).is_err());
    assert!(c.deck_play(2, signal(0.2, 8000), 0.0).is_err());
    c.deck_play(0, signal(f32::NAN, 8000), 0.0).unwrap();
    assert!(block(&mut r).iter().all(|s| s.is_finite()));
}

#[test]
fn a_fader_moved_while_paused_cannot_flash_a_hidden_deck_on_resume() {
    let (mut r, mut c) = setup();
    let source = signal(0.4, 8000);
    c.deck_mix(1.0, [1.0; 2], [false; 2]).unwrap();
    c.deck_play(1, source.clone(), 0.0).unwrap();
    block(&mut r);
    c.deck_pause(1).unwrap();
    block(&mut r);
    c.deck_mix(0.0, [1.0; 2], [false; 2]).unwrap();
    block(&mut r);
    c.deck_play(1, source, 0.0).unwrap();
    assert!(block(&mut r).iter().all(|s| *s == 0.0));
}

fn sine(hz: f32, rate: u32) -> Arc<AudioData> {
    let samples = (0..rate as usize * 4)
        .flat_map(|i| {
            let s = 0.5 * (std::f32::consts::TAU * hz * i as f32 / rate as f32).sin();
            [s, s]
        })
        .collect();
    Arc::new(AudioData { sample_rate: rate, channels: 2, samples })
}
/// The loudest sample of a block's second half, once the filters have settled.
fn peak(r: &mut Renderer) -> f32 {
    block(r);
    block(r)[1024..].iter().fold(0.0f32, |m, s| m.max(s.abs()))
}
#[test]
fn a_deck_plays_faster_or_slower_and_its_clock_follows() {
    let (mut r, mut c) = setup();
    c.deck_rate(0, 1.5).unwrap();
    c.deck_play(0, signal(0.2, 8000), 0.0).unwrap();
    block(&mut r);
    assert!((c.deck_monitor(0).unwrap().seconds() - 0.192).abs() < 1e-6);
    c.deck_rate(0, 1.0).unwrap();
    block(&mut r);
    assert!((c.deck_monitor(0).unwrap().seconds() - 0.32).abs() < 1e-6);
    for wrong in [0.49, 2.01, f64::NAN, f64::INFINITY] {
        assert!(c.deck_rate(0, wrong).is_err(), "{wrong}");
    }
    assert!(c.deck_rate(2, 1.0).is_err());
}
#[test]
fn a_deck_eq_kills_its_band_and_the_filter_closes_from_either_side() {
    let (mut r, mut c) = setup();
    c.deck_mix(0.0, [1.0; 2], [false; 2]).unwrap();
    c.deck_play(0, sine(80.0, 8000), 0.0).unwrap();
    let flat = peak(&mut r);
    assert!((flat - 0.5 * 0.501_187_2).abs() < 0.01, "{flat}");
    c.deck_tone(0, DeckTone { eq: [DECK_KILL_DB, 0.0, 0.0], filter: 0.0 }).unwrap();
    assert!(peak(&mut r) < flat * 0.1);
    c.deck_tone(0, DeckTone { eq: [0.0, 0.0, DECK_KILL_DB], filter: 0.0 }).unwrap();
    assert!(peak(&mut r) > flat * 0.9, "the high band leaves a bass alone");
    c.deck_tone(0, DeckTone { eq: [0.0; 3], filter: 1.0 }).unwrap();
    assert!(peak(&mut r) < flat * 0.1, "the high-pass turned fully removes a bass");
    c.deck_tone(0, DeckTone { eq: [0.0; 3], filter: -1.0 }).unwrap();
    assert!(peak(&mut r) > flat * 0.8, "the low-pass keeps a bass");
    c.deck_play(0, sine(2500.0, 8000), 0.0).unwrap();
    assert!(peak(&mut r) < flat * 0.1, "the low-pass turned fully removes a high tone");
    c.deck_tone(0, DeckTone::default()).unwrap();
    assert!((peak(&mut r) - flat).abs() < 0.02);
    for wrong in [
        DeckTone { eq: [-41.0, 0.0, 0.0], filter: 0.0 },
        DeckTone { eq: [0.0, 7.0, 0.0], filter: 0.0 },
        DeckTone { eq: [0.0; 3], filter: 1.5 },
        DeckTone { eq: [f32::NAN, 0.0, 0.0], filter: 0.0 },
    ] {
        assert!(c.deck_tone(0, wrong).is_err(), "{wrong:?}");
    }
    assert!(c.deck_tone(2, DeckTone::default()).is_err());
}
