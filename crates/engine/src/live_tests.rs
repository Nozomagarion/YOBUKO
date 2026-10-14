//! Live input (ADR 0017): notes from a MIDI controller played by the armed tracks' instruments,
//! with the transport stopped or running, across graph swaps.

use suhina_model::*;

use crate::chain_tests::{SR, blocks, device, project};
use crate::{AudioPool, LIVE_QUEUE, LaunchCommand, LiveEvent, Renderer};

fn synth() -> Project {
    project(Vec::new(), 1.0, vec![device("fm-synth", &[])])
}

/// Two tracks with an instrument each, on channels 1 and 2.
fn two_synths() -> Project {
    let mut p = synth();
    let mut track = p.tracks[0].clone();
    track.id = TrackId(2);
    track.channel = Some(ChannelId(2));
    p.tracks.push(track);
    let mut channel = p.channels[0].clone();
    channel.id = ChannelId(2);
    p.channels.push(channel);
    p
}

fn stopped(project: &Project) -> Renderer {
    Renderer::stopped(project, &AudioPool::new(), SR).unwrap()
}

fn peak(out: &[f32]) -> f32 {
    out.iter().fold(0.0, |m, s| m.max(s.abs()))
}

#[test]
fn an_armed_tracks_instrument_plays_live_notes_with_the_transport_stopped() {
    let mut renderer = stopped(&synth());
    let mut controller = renderer.controller();
    let mut live = controller.take_live_input().unwrap();
    assert!(controller.take_live_input().is_none(), "taken once");
    controller.send(LaunchCommand::SetArmed { track: TrackId(1), armed: true }).unwrap();
    assert_eq!(peak(&blocks(&mut renderer, 2, 256)), 0.0, "silent until a key goes down");

    assert!(live.send(LiveEvent::NoteOn { key: 60, velocity: 1.0 }));
    let first = blocks(&mut renderer, 1, 256);
    // Heard from the first frame of the next block.
    assert!(peak(first.get(..32).unwrap()) > 0.0, "the note starts at once");
    let held = peak(&blocks(&mut renderer, 20, 256));
    assert!(held > 0.01, "{held}");

    assert!(live.send(LiveEvent::NoteOff { key: 60 }));
    let after = blocks(&mut renderer, 400, 256);
    let late = peak(&after[380 * 512..]);
    assert!(late < held * 0.05, "released: {late} of {held}");
}

#[test]
fn a_track_that_is_not_armed_hears_nothing() {
    let mut renderer = stopped(&synth());
    let mut controller = renderer.controller();
    let mut live = controller.take_live_input().unwrap();
    live.send(LiveEvent::NoteOn { key: 60, velocity: 1.0 });
    assert_eq!(peak(&blocks(&mut renderer, 10, 256)), 0.0);
}

#[test]
fn every_armed_track_plays_and_a_disarmed_one_lets_go() {
    let mut both = stopped(&two_synths());
    let mut controller = both.controller();
    let mut live = controller.take_live_input().unwrap();
    controller.send(LaunchCommand::SetArmed { track: TrackId(1), armed: true }).unwrap();
    controller.send(LaunchCommand::SetArmed { track: TrackId(2), armed: true }).unwrap();
    live.send(LiveEvent::NoteOn { key: 64, velocity: 1.0 });
    let two = peak(&blocks(&mut both, 20, 256));

    let mut one = stopped(&two_synths());
    let mut controller_one = one.controller();
    let mut live_one = controller_one.take_live_input().unwrap();
    controller_one.send(LaunchCommand::SetArmed { track: TrackId(1), armed: true }).unwrap();
    live_one.send(LiveEvent::NoteOn { key: 64, velocity: 1.0 });
    let single = peak(&blocks(&mut one, 20, 256));
    assert!(two > single * 1.5, "two instruments are louder than one: {two} {single}");

    // Disarming both releases the key still held down.
    controller.send(LaunchCommand::SetArmed { track: TrackId(1), armed: false }).unwrap();
    controller.send(LaunchCommand::SetArmed { track: TrackId(2), armed: false }).unwrap();
    let after = blocks(&mut both, 400, 256);
    assert!(peak(&after[380 * 512..]) < two * 0.05);
}

#[test]
fn a_track_armed_while_a_key_is_down_hears_the_next_key() {
    let mut renderer = stopped(&synth());
    let mut controller = renderer.controller();
    let mut live = controller.take_live_input().unwrap();
    live.send(LiveEvent::NoteOn { key: 60, velocity: 1.0 });
    blocks(&mut renderer, 1, 256);
    controller.send(LaunchCommand::SetArmed { track: TrackId(1), armed: true }).unwrap();
    assert_eq!(peak(&blocks(&mut renderer, 4, 256)), 0.0, "the key already down is not started");
    live.send(LiveEvent::NoteOn { key: 62, velocity: 1.0 });
    assert!(peak(&blocks(&mut renderer, 4, 256)) > 0.0);
}

#[test]
fn a_key_held_across_a_graph_swap_is_released_by_its_note_off() {
    let project = synth();
    let mut renderer = stopped(&project);
    let mut controller = renderer.controller();
    let mut live = controller.take_live_input().unwrap();
    controller.send(LaunchCommand::SetArmed { track: TrackId(1), armed: true }).unwrap();
    live.send(LiveEvent::NoteOn { key: 60, velocity: 1.0 });
    let held = peak(&blocks(&mut renderer, 20, 256));

    // An edit that keeps the chain: the instrument and its held note carry over.
    let mut edited = project.clone();
    edited.channels[0].volume = 0.9;
    controller.update(&edited).unwrap();
    blocks(&mut renderer, 4, 256);
    // The new graph's track is still armed.
    live.send(LiveEvent::NoteOn { key: 67, velocity: 1.0 });
    assert!(peak(&blocks(&mut renderer, 4, 256)) > 0.0);
    live.send(LiveEvent::NoteOff { key: 60 });
    live.send(LiveEvent::NoteOff { key: 67 });
    let after = blocks(&mut renderer, 400, 256);
    assert!(peak(&after[380 * 512..]) < held * 0.05, "nothing hangs after the swap");
}

#[test]
fn all_notes_off_releases_every_key() {
    let mut renderer = stopped(&synth());
    let mut controller = renderer.controller();
    let mut live = controller.take_live_input().unwrap();
    controller.send(LaunchCommand::SetArmed { track: TrackId(1), armed: true }).unwrap();
    for key in [48, 52, 55, 60] {
        live.send(LiveEvent::NoteOn { key, velocity: 0.8 });
    }
    let held = peak(&blocks(&mut renderer, 20, 256));
    live.send(LiveEvent::AllNotesOff);
    let after = blocks(&mut renderer, 400, 256);
    assert!(peak(&after[380 * 512..]) < held * 0.05);
}

#[test]
fn a_full_queue_drops_notes_and_a_gone_renderer_is_noticed() {
    let mut renderer = stopped(&synth());
    let mut controller = renderer.controller();
    let mut live = controller.take_live_input().unwrap();
    controller.send(LaunchCommand::SetArmed { track: TrackId(1), armed: true }).unwrap();
    let mut sent = 0;
    for i in 0..LIVE_QUEUE + 100 {
        let key = (i % 128) as u8;
        let event = if i % 2 == 0 { LiveEvent::NoteOn { key, velocity: 1.0 } } else { LiveEvent::NoteOff { key } };
        if live.send(event) {
            sent += 1;
        }
    }
    assert_eq!(sent, LIVE_QUEUE);
    assert!(blocks(&mut renderer, 4, 256).iter().all(|s| s.is_finite()));
    assert!(live.is_connected());
    drop(renderer);
    assert!(!live.is_connected());
    assert!(!live.send(LiveEvent::NoteOn { key: 60, velocity: 1.0 }));
}

#[test]
fn hostile_live_values_are_made_safe() {
    let mut renderer = stopped(&synth());
    let mut controller = renderer.controller();
    let mut live = controller.take_live_input().unwrap();
    controller.send(LaunchCommand::SetArmed { track: TrackId(1), armed: true }).unwrap();
    controller.send(LaunchCommand::SetArmed { track: TrackId(99), armed: true }).unwrap();
    controller.send(LaunchCommand::SetArmed { track: TrackId(99), armed: false }).unwrap();
    controller.send(LaunchCommand::SetArmed { track: TrackId(5), armed: false }).unwrap();
    live.send(LiveEvent::NoteOn { key: 200, velocity: 1.0 });
    live.send(LiveEvent::NoteOff { key: 255 });
    live.send(LiveEvent::NoteOn { key: 60, velocity: f32::NAN });
    live.send(LiveEvent::NoteOn { key: 61, velocity: f32::INFINITY });
    live.send(LiveEvent::NoteOn { key: 62, velocity: -3.0 });
    let out = blocks(&mut renderer, 8, 256);
    assert!(out.iter().all(|s| s.is_finite()));
    assert!(peak(&out) > 0.0);
}

#[test]
fn more_tracks_than_can_be_armed_are_left_unarmed() {
    let mut renderer = stopped(&synth());
    let mut controller = renderer.controller();
    for id in 100..100 + crate::MAX_ARMED as u32 {
        controller.send(LaunchCommand::SetArmed { track: TrackId(id), armed: true }).unwrap();
    }
    blocks(&mut renderer, 1, 256);
    let mut live = controller.take_live_input().unwrap();
    controller.send(LaunchCommand::SetArmed { track: TrackId(1), armed: true }).unwrap();
    live.send(LiveEvent::NoteOn { key: 60, velocity: 1.0 });
    assert_eq!(peak(&blocks(&mut renderer, 4, 256)), 0.0, "no room for track 1");
    controller.send(LaunchCommand::SetArmed { track: TrackId(100), armed: false }).unwrap();
    controller.send(LaunchCommand::SetArmed { track: TrackId(1), armed: true }).unwrap();
    live.send(LiveEvent::NoteOn { key: 62, velocity: 1.0 });
    assert!(peak(&blocks(&mut renderer, 4, 256)) > 0.0);
}

#[test]
fn the_sustain_pedal_holds_released_keys_until_it_comes_up() {
    let mut renderer = stopped(&synth());
    let mut controller = renderer.controller();
    let mut live = controller.take_live_input().unwrap();
    controller.send(LaunchCommand::SetArmed { track: TrackId(1), armed: true }).unwrap();
    live.send(LiveEvent::Sustain { on: true });
    live.send(LiveEvent::NoteOn { key: 60, velocity: 1.0 });
    let held = peak(&blocks(&mut renderer, 20, 256));
    live.send(LiveEvent::NoteOff { key: 60 });
    let sustained = blocks(&mut renderer, 400, 256);
    assert!(peak(&sustained[380 * 512..]) > held * 0.05, "the pedal keeps it sounding");

    live.send(LiveEvent::Sustain { on: false });
    let after = blocks(&mut renderer, 400, 256);
    assert!(peak(&after[380 * 512..]) < held * 0.05, "released with the pedal");

    // A key still held down when the pedal comes up keeps sounding.
    live.send(LiveEvent::Sustain { on: true });
    live.send(LiveEvent::NoteOn { key: 64, velocity: 1.0 });
    live.send(LiveEvent::Sustain { on: false });
    let still = blocks(&mut renderer, 400, 256);
    assert!(peak(&still[380 * 512..]) > held * 0.05);
    live.send(LiveEvent::NoteOff { key: 64 });
    let gone = blocks(&mut renderer, 400, 256);
    assert!(peak(&gone[380 * 512..]) < held * 0.05);
}

#[test]
fn the_renderer_says_whether_a_track_is_armed() {
    let mut renderer = stopped(&synth());
    let mut controller = renderer.controller();
    assert!(!renderer.live_armed());
    controller.send(LaunchCommand::SetArmed { track: TrackId(1), armed: true }).unwrap();
    blocks(&mut renderer, 1, 64);
    assert!(renderer.live_armed());
    controller.send(LaunchCommand::SetArmed { track: TrackId(1), armed: false }).unwrap();
    blocks(&mut renderer, 1, 64);
    assert!(!renderer.live_armed());
}

#[test]
fn a_live_note_is_heard_from_the_start_of_the_next_block_at_any_block_size() {
    // The engine's part of a key's latency is at most one block: the note waits for the next
    // block and sounds from its first frames, whatever the block size.
    for frames in [64, 128, 256, 1024] {
        let mut renderer = stopped(&synth());
        let mut controller = renderer.controller();
        let mut live = controller.take_live_input().unwrap();
        controller.send(LaunchCommand::SetArmed { track: TrackId(1), armed: true }).unwrap();
        assert_eq!(peak(&blocks(&mut renderer, 1, frames)), 0.0);
        live.send(LiveEvent::NoteOn { key: 69, velocity: 1.0 });
        let out = blocks(&mut renderer, 1, frames);
        let first = out.iter().position(|s| s.abs() > 1e-6).map(|i| i / 2);
        assert!(first.is_some_and(|f| f < 8), "{frames}: first sound at {first:?}");
    }
}
