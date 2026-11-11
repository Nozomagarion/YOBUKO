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
        c.deck_mix(DeckMix { crossfade: x, levels: [1.0; 2], cue: [false; 2], ..DeckMix::default() }).unwrap();
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
    c.deck_mix(DeckMix { crossfade: 0.0, levels: [1.0; 2], cue: [false, true], ..DeckMix::default() }).unwrap();
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
    assert!(
        c.deck_mix(DeckMix { crossfade: f32::NAN, levels: [1.0; 2], cue: [false; 2], ..DeckMix::default() }).is_err()
    );
    assert!(c.deck_play(2, signal(0.2, 8000), 0.0).is_err());
    c.deck_play(0, signal(f32::NAN, 8000), 0.0).unwrap();
    assert!(block(&mut r).iter().all(|s| s.is_finite()));
}

#[test]
fn a_fader_moved_while_paused_cannot_flash_a_hidden_deck_on_resume() {
    let (mut r, mut c) = setup();
    let source = signal(0.4, 8000);
    c.deck_mix(DeckMix { crossfade: 1.0, levels: [1.0; 2], cue: [false; 2], ..DeckMix::default() }).unwrap();
    c.deck_play(1, source.clone(), 0.0).unwrap();
    block(&mut r);
    c.deck_pause(1).unwrap();
    block(&mut r);
    c.deck_mix(DeckMix { crossfade: 0.0, levels: [1.0; 2], cue: [false; 2], ..DeckMix::default() }).unwrap();
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
    c.deck_mix(DeckMix { crossfade: 0.0, levels: [1.0; 2], cue: [false; 2], ..DeckMix::default() }).unwrap();
    c.deck_play(0, sine(80.0, 8000), 0.0).unwrap();
    let flat = peak(&mut r);
    assert!((flat - 0.5 * 0.501_187_2).abs() < 0.01, "{flat}");
    c.deck_tone(0, DeckTone { eq: [DECK_KILL_DB, 0.0, 0.0], filter: 0.0, trim: 0.0 }).unwrap();
    assert!(peak(&mut r) < flat * 0.1);
    c.deck_tone(0, DeckTone { eq: [0.0, 0.0, DECK_KILL_DB], filter: 0.0, trim: 0.0 }).unwrap();
    assert!(peak(&mut r) > flat * 0.9, "the high band leaves a bass alone");
    c.deck_tone(0, DeckTone { eq: [0.0; 3], filter: 1.0, trim: 0.0 }).unwrap();
    assert!(peak(&mut r) < flat * 0.1, "the high-pass turned fully removes a bass");
    c.deck_tone(0, DeckTone { eq: [0.0; 3], filter: -1.0, trim: 0.0 }).unwrap();
    assert!(peak(&mut r) > flat * 0.8, "the low-pass keeps a bass");
    c.deck_play(0, sine(2500.0, 8000), 0.0).unwrap();
    assert!(peak(&mut r) < flat * 0.1, "the low-pass turned fully removes a high tone");
    c.deck_tone(0, DeckTone::default()).unwrap();
    assert!((peak(&mut r) - flat).abs() < 0.02);
    for wrong in [
        DeckTone { eq: [-41.0, 0.0, 0.0], filter: 0.0, trim: 0.0 },
        DeckTone { eq: [0.0, 7.0, 0.0], filter: 0.0, trim: 0.0 },
        DeckTone { eq: [0.0; 3], filter: 1.5, trim: 0.0 },
        DeckTone { eq: [f32::NAN, 0.0, 0.0], filter: 0.0, trim: 0.0 },
    ] {
        assert!(c.deck_tone(0, wrong).is_err(), "{wrong:?}");
    }
    assert!(c.deck_tone(2, DeckTone::default()).is_err());
}
/// A track whose every sample says where it is: frame `i` holds `i / frames`.
fn ramp(rate: u32, seconds: usize) -> Arc<AudioData> {
    let frames = rate as usize * seconds;
    let samples = (0..frames).flat_map(|i| [i as f32 / frames as f32; 2]).collect();
    Arc::new(AudioData { sample_rate: rate, channels: 2, samples })
}
#[test]
fn a_deck_loops_round_and_round_until_the_loop_is_let_go() {
    let (mut r, mut c) = setup();
    c.deck_mix(DeckMix { crossfade: 0.0, ..DeckMix::default() }).unwrap();
    c.deck_play(0, ramp(8000, 4), 0.0).unwrap();
    c.deck_loop(0, Some((0.25, 0.5))).unwrap();
    for _ in 0..20 {
        block(&mut r);
        let at = c.deck_monitor(0).unwrap().seconds();
        assert!(at < 0.5 + 1e-9, "{at}");
    }
    assert!(c.deck_monitor(0).unwrap().playing());
    // Round the loop, the sound carries on with no gap: never silent inside it.
    let b = block(&mut r);
    assert!(b.iter().all(|s| *s > 0.25 / 4.0 * 0.501_187_2 * 0.9), "a loop has no seam of silence");
    c.deck_loop(0, None).unwrap();
    for _ in 0..4 {
        block(&mut r);
    }
    assert!(c.deck_monitor(0).unwrap().seconds() > 0.5);
    // A seek out of the loop plays on from there; a loop set behind the playhead waits.
    c.deck_loop(0, Some((0.25, 0.5))).unwrap();
    c.deck_seek(0, 2.0).unwrap();
    block(&mut r);
    assert!(c.deck_monitor(0).unwrap().seconds() > 2.0);
    for wrong in [Some((0.5, 0.5)), Some((-1.0, 0.5)), Some((0.0, f64::NAN))] {
        assert!(c.deck_loop(0, wrong).is_err(), "{wrong:?}");
    }
    assert!(c.deck_loop(2, None).is_err());
}
#[test]
fn a_nudge_speeds_a_deck_up_or_slows_it_down_while_it_is_held() {
    let (mut r, mut c) = setup();
    c.deck_play(0, signal(0.2, 8000), 0.0).unwrap();
    c.deck_bend(0, 0.25).unwrap();
    block(&mut r);
    assert!((c.deck_monitor(0).unwrap().seconds() - 0.16).abs() < 1e-6);
    c.deck_bend(0, -0.25).unwrap();
    block(&mut r);
    assert!((c.deck_monitor(0).unwrap().seconds() - 0.256).abs() < 1e-6);
    c.deck_bend(0, 0.0).unwrap();
    block(&mut r);
    assert!((c.deck_monitor(0).unwrap().seconds() - 0.384).abs() < 1e-6);
    assert!(c.deck_bend(0, 0.3).is_err());
}
#[test]
fn trim_master_and_meters_follow_the_mixer() {
    let (mut r, mut c) = setup();
    c.deck_mix(DeckMix { crossfade: 0.0, ..DeckMix::default() }).unwrap();
    c.deck_play(0, signal(0.25, 8000), 0.0).unwrap();
    block(&mut r);
    let b = block(&mut r);
    assert!((b[1600] - 0.25 * 0.501_187_2).abs() < 1e-5);
    let meter = c.deck_monitor(0).unwrap();
    let [l, right] = meter.meter.take();
    assert!((l - 0.25).abs() < 1e-5 && (right - 0.25).abs() < 1e-5, "the channel meter is before the headroom");
    assert_eq!(meter.meter.take(), [0.0; 2], "a meter read is reset");
    let master = c.deck_master_meter();
    assert!((master.take()[0] - 0.25 * 0.501_187_2).abs() < 1e-5);
    // +6 dB of trim doubles it; half the master halves it again.
    c.deck_tone(0, DeckTone { trim: 6.020_6, ..DeckTone::default() }).unwrap();
    c.deck_mix(DeckMix { crossfade: 0.0, master: 0.5, ..DeckMix::default() }).unwrap();
    block(&mut r);
    let b = block(&mut r);
    assert!((b[1600] - 0.25 * 0.501_187_2).abs() < 1e-4, "{}", b[1600]);
    assert!((meter.meter.take()[0] - 0.5).abs() < 1e-3);
    assert!(c.deck_tone(0, DeckTone { trim: 13.0, ..DeckTone::default() }).is_err());
    assert!(c.deck_mix(DeckMix { master: 1.5, ..DeckMix::default() }).is_err());
}
#[test]
fn the_headphones_blend_the_cued_deck_with_the_master() {
    let (mut r, mut c) = setup();
    c.send(LaunchCommand::SetCue { gain: Some(1.0), separate: Some(true) }).unwrap();
    c.deck_play(0, signal(0.2, 8000), 0.0).unwrap();
    c.deck_play(1, signal(0.4, 8000), 0.0).unwrap();
    let mut heard = |c: &mut Controller, mix: DeckMix| {
        c.deck_mix(mix).unwrap();
        let (mut main, mut cue) = (vec![0.0; 2048], vec![0.0; 2048]);
        r.process_with_cue(&mut main, &mut cue);
        let (mut main, mut cue) = (vec![0.0; 2048], vec![0.0; 2048]);
        r.process_with_cue(&mut main, &mut cue);
        (main[1600], cue[1600])
    };
    // Deck B cued, all the way to "cue": only B in the headphones, A only on the master.
    let (main, head) = heard(&mut c, DeckMix { crossfade: 0.0, cue: [false, true], ..DeckMix::default() });
    assert!((main - 0.2 * 0.501_187_2).abs() < 1e-5);
    assert!((head - 0.4 * 0.501_187_2).abs() < 1e-5, "{head}");
    // All the way to "master": the headphones hear what the room hears.
    let (main, head) =
        heard(&mut c, DeckMix { crossfade: 0.0, cue: [false, true], phones_mix: 1.0, ..DeckMix::default() });
    assert!((head - main).abs() < 1e-5, "{head} {main}");
    // The headphones' level.
    let (_, head) = heard(&mut c, DeckMix { crossfade: 0.0, cue: [false, true], phones: 0.5, ..DeckMix::default() });
    assert!((head - 0.2 * 0.501_187_2).abs() < 1e-5, "{head}");
}
/// The pitch of the left side over the next `blocks` blocks, from its rising zero crossings.
fn pitch(r: &mut Renderer, blocks: usize) -> f64 {
    let left: Vec<f32> = (0..blocks).flat_map(|_| block(r).into_iter().step_by(2)).collect();
    let rising = left.windows(2).filter(|w| w[0] <= 0.0 && w[1] > 0.0).count();
    rising as f64 / (left.len() as f64 / 8000.0)
}
#[test]
fn the_key_lock_keeps_the_pitch_when_the_speed_changes_and_the_key_moves_it() {
    let (mut r, mut c) = setup();
    c.deck_rate(0, 1.5).unwrap();
    c.deck_play(0, sine(440.0, 8000), 0.0).unwrap();
    let free = pitch(&mut r, 4);
    assert!((free - 660.0).abs() < 8.0, "{free}");
    c.deck_key_lock(0, true).unwrap();
    block(&mut r);
    let at = c.deck_monitor(0).unwrap().seconds();
    let locked = pitch(&mut r, 4);
    assert!((locked - 440.0).abs() < 8.0, "{locked}");
    // The clock still runs at the deck's speed: 4 blocks of 0.128 s, 1.5 times as fast.
    let moved = c.deck_monitor(0).unwrap().seconds() - at;
    assert!((moved - 0.768).abs() < 1e-6, "{moved}");
    c.deck_rate(0, 1.0).unwrap();
    c.deck_key(0, 12.0).unwrap();
    block(&mut r);
    let up = pitch(&mut r, 4);
    assert!((up - 880.0).abs() < 12.0, "{up}");
    // Unlocked and a key moved: the speed and the key both move the pitch.
    c.deck_key_lock(0, false).unwrap();
    c.deck_rate(0, 0.75).unwrap();
    c.deck_key(0, -12.0).unwrap();
    c.deck_seek(0, 0.5).unwrap();
    block(&mut r);
    let down = pitch(&mut r, 4);
    assert!((down - 165.0).abs() < 6.0, "{down}");
    for wrong in [12.01, -12.01, f64::NAN] {
        assert!(c.deck_key(0, wrong).is_err(), "{wrong}");
    }
}
#[test]
fn a_locked_deck_starts_at_once_and_loops_without_a_gap() {
    let (mut r, mut c) = setup();
    c.deck_key_lock(0, true).unwrap();
    c.deck_rate(0, 1.25).unwrap();
    c.deck_play(0, sine(200.0, 8000), 1.0).unwrap();
    let first = block(&mut r);
    // Sound from the first few milliseconds, not after the stretcher's latency.
    assert!(first[..2 * 80].iter().any(|s| s.abs() > 0.05));
    c.deck_loop(0, Some((1.0, 1.5))).unwrap();
    for _ in 0..12 {
        let b = block(&mut r);
        let seconds = c.deck_monitor(0).unwrap().seconds();
        assert!((1.0..1.5).contains(&seconds), "{seconds}");
        // No silent hole: every 20 ms of the block has sound.
        for piece in b.chunks(2 * 160) {
            assert!(piece.iter().any(|s| s.abs() > 0.05));
        }
    }
}
#[test]
fn a_shift_moves_a_deck_from_wherever_it_is_when_it_lands() {
    let (mut r, mut c) = setup();
    c.deck_play(0, signal(0.2, 8000), 1.0).unwrap();
    block(&mut r);
    // Measured a moment ago; the deck has moved on since, and the shift adds to where it is.
    c.deck_shift(0, -0.25).unwrap();
    block(&mut r);
    let seconds = c.deck_monitor(0).unwrap().seconds();
    assert!((seconds - (1.0 + 0.128 - 0.25 + 0.128)).abs() < 1e-6, "{seconds}");
    assert!(c.deck_shift(0, f64::NAN).is_err());
}
fn click(rate: u32) -> Arc<AudioData> {
    let mut samples = vec![0.0; rate as usize * 4 * 2];
    // After the deck's and the effect's fade in.
    samples[800] = 0.8;
    samples[801] = 0.8;
    Arc::new(AudioData { sample_rate: rate, channels: 2, samples })
}
fn unit(kind: DeckFxKind, meta: f32, targets: [bool; 3]) -> DeckFxUnit {
    let mut unit = DeckFxUnit { mix: 1.0, beats: 0.5, bpm: 120.0, targets, ..DeckFxUnit::default() };
    unit.slots[0] = DeckFxSlot { kind: Some(kind), on: true, meta };
    unit
}
#[test]
fn an_echo_repeats_the_deck_a_number_of_beats_later_and_rings_out_when_switched_off() {
    let (mut r, mut c) = setup();
    c.deck_mix(DeckMix { crossfade: 0.0, ..DeckMix::default() }).unwrap();
    // Half a beat at 120 BPM: a quarter second, 2000 frames at 8 kHz.
    c.deck_fx(0, unit(DeckFxKind::Echo, 0.5, [true, false, false])).unwrap();
    c.deck_play(0, click(8000), 0.0).unwrap();
    let left: Vec<f32> = (0..5).flat_map(|_| block(&mut r).into_iter().step_by(2)).collect();
    let loudest_after =
        |from: usize, to: usize| (from..to).max_by(|a, b| left[*a].abs().total_cmp(&left[*b].abs())).unwrap();
    let echo = loudest_after(500, 3400);
    assert!((2395..=2405).contains(&echo), "the echo is at {echo}");
    assert!(left[echo].abs() > 0.05, "{}", left[echo]);
    let second = loudest_after(3400, 5120);
    assert!((4395..=4405).contains(&second) && left[second].abs() < left[echo].abs(), "{second}");
    // Off: nothing new goes in, but what it holds still rings.
    let mut off = unit(DeckFxKind::Echo, 0.5, [true, false, false]);
    off.slots[0].on = false;
    c.deck_fx(0, off).unwrap();
    let tail: Vec<f32> = (0..2).flat_map(|_| block(&mut r)).collect();
    assert!(tail.iter().any(|s| s.abs() > 0.001), "the echo rings out");
}
#[test]
fn a_filter_effect_closes_and_goes_only_where_its_unit_is_put() {
    let (mut r, mut c) = setup();
    c.deck_play(0, sine(2000.0, 8000), 0.0).unwrap();
    let open = peak(&mut r);
    c.deck_fx(1, unit(DeckFxKind::Filter, 0.0, [false, true, false])).unwrap();
    assert!((peak(&mut r) - open).abs() < 0.01, "on deck B, deck A is untouched");
    c.deck_fx(1, unit(DeckFxKind::Filter, 0.0, [true, false, false])).unwrap();
    let closed = peak(&mut r);
    assert!(closed < open * 0.1, "{closed} vs {open}");
    // On the master it filters too.
    c.deck_fx(1, unit(DeckFxKind::Filter, 0.0, [false, false, true])).unwrap();
    assert!(peak(&mut r) < open * 0.1);
    let mut wrong = unit(DeckFxKind::Filter, 0.0, [true, false, false]);
    wrong.beats = 64.0;
    assert!(c.deck_fx(0, wrong).is_err());
    wrong.beats = 1.0;
    wrong.mix = 1.5;
    assert!(c.deck_fx(0, wrong).is_err());
    assert!(c.deck_fx(2, DeckFxUnit::default()).is_err());
}
#[test]
fn every_effect_plays_on_a_deck_without_blowing_up() {
    for kind in DeckFxKind::ALL {
        let (mut r, mut c) = setup();
        c.deck_fx(0, unit(kind, 0.8, [true, false, false])).unwrap();
        c.deck_play(0, sine(440.0, 8000), 0.0).unwrap();
        let mut loudest = 0.0f32;
        for _ in 0..8 {
            for s in block(&mut r) {
                assert!(s.is_finite(), "{kind:?}");
                loudest = loudest.max(s.abs());
            }
        }
        assert!(loudest > 0.01 && loudest < 2.0, "{kind:?}: {loudest}");
        assert_eq!(DeckFxKind::from_id(kind.id()), Some(kind));
    }
}
#[test]
fn a_sampler_slot_plays_from_its_start_into_the_master_at_its_level() {
    let (mut r, mut c) = setup();
    c.sampler_play(3, signal(0.4, 8000)).unwrap();
    let out = block(&mut r);
    assert!((out[1600] - 0.4 * 0.501_187_2).abs() < 1e-5, "{}", out[1600]);
    assert!(c.sampler_monitor(3).unwrap().playing());
    // Played again, it starts over.
    block(&mut r);
    c.sampler_play(3, signal(0.4, 8000)).unwrap();
    block(&mut r);
    assert!((c.sampler_monitor(3).unwrap().seconds() - 0.128).abs() < 1e-6);
    // Its level, then stopped.
    c.sampler_gain(3, 0.5).unwrap();
    block(&mut r);
    let out = block(&mut r);
    assert!((out[1600] - 0.2 * 0.501_187_2).abs() < 1e-5, "{}", out[1600]);
    // The master level and the master's effects follow it too.
    c.deck_mix(DeckMix { master: 0.5, ..DeckMix::default() }).unwrap();
    block(&mut r);
    let out = block(&mut r);
    assert!((out[1600] - 0.1 * 0.501_187_2).abs() < 1e-5, "{}", out[1600]);
    c.sampler_stop(3).unwrap();
    assert!(block(&mut r).iter().all(|s| *s == 0.0));
    assert!(!c.sampler_monitor(3).unwrap().playing());
    assert!(c.sampler_play(8, signal(0.4, 8000)).is_err());
    assert!(c.sampler_gain(0, 1.5).is_err());
}
