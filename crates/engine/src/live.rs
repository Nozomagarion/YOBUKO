//! Live input ([ADR 0017](../../../docs/decisions/0017-inputs-and-latency.md)): notes played on a
//! MIDI controller, heard through the instrument of every armed track, whether the transport
//! runs or not.
//!
//! The MIDI driver's thread sends notes over a lock-free queue; the render thread takes them at
//! the start of each block and plays them at its first frame on the chains of the armed tracks,
//! the way the preview voice plays its notes (ADR 0011). Which tracks are armed comes from the
//! controller ([`crate::LaunchCommand::SetArmed`]) and outlives graph swaps. Nothing here
//! allocates, locks or frees on the render thread.

use rtrb::{Consumer, Producer, RingBuffer};
use suhina_model::TrackId;

use crate::chain::Chain;

/// Notes waiting for the render thread: more than a hand plays within one block of any size.
pub const LIVE_QUEUE: usize = 1024;
/// Tracks that can be armed at once; arming more leaves the rest unarmed.
pub const MAX_ARMED: usize = 64;
/// A live note's end: it sounds until its key is released.
const UNTIL_RELEASED: u64 = u64::MAX - 1;

/// What a MIDI controller played.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum LiveEvent {
    /// A key went down; `velocity` is from 0 to 1 (a note on of velocity 0 is a [`LiveEvent::NoteOff`]).
    NoteOn {
        key: u8,
        velocity: f32,
    },
    NoteOff {
        key: u8,
    },
    /// The sustain pedal went down or up: while it is down, released keys keep sounding until
    /// it comes up.
    Sustain {
        on: bool,
    },
    /// Every key the controller holds goes up (a closed port, an all-notes-off message).
    AllNotesOff,
}

/// The sending end of the live-note queue, for the thread a MIDI driver calls back on.
pub struct LiveSender {
    producer: Producer<LiveEvent>,
}

impl LiveSender {
    /// Queues `event` for the start of the next block. Returns false when it was dropped: the
    /// queue is full, or the renderer it fed is gone.
    pub fn send(&mut self, event: LiveEvent) -> bool {
        !self.producer.is_abandoned() && self.producer.push(event).is_ok()
    }

    /// The renderer still takes notes from this sender.
    pub fn is_connected(&self) -> bool {
        !self.producer.is_abandoned()
    }
}

impl std::fmt::Debug for LiveSender {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("LiveSender").field("connected", &self.is_connected()).finish()
    }
}

/// The render thread's end: the queue, the armed tracks and the keys held down.
pub(crate) struct Live {
    consumer: Option<Consumer<LiveEvent>>,
    armed: [TrackId; MAX_ARMED],
    armed_count: usize,
    /// The keys the controller holds down now.
    held: [bool; 128],
    /// The sustain pedal is down.
    sustain: bool,
    /// Keys released while the pedal is down, sounding until it comes up.
    sustained: [bool; 128],
}

impl Live {
    pub(crate) fn new() -> Self {
        Self {
            consumer: None,
            armed: [TrackId(0); MAX_ARMED],
            armed_count: 0,
            held: [false; 128],
            sustain: false,
            sustained: [false; 128],
        }
    }

    /// A new queue to the renderer; the previous sender is cut off.
    pub(crate) fn sender(&mut self) -> LiveSender {
        let (producer, consumer) = RingBuffer::new(LIVE_QUEUE);
        self.consumer = Some(consumer);
        LiveSender { producer }
    }

    fn armed(&self) -> &[TrackId] {
        self.armed.get(..self.armed_count).unwrap_or_default()
    }

    /// Whether any track is armed: the output then keeps a shallower queue (ADR 0017).
    pub(crate) fn any_armed(&self) -> bool {
        self.armed_count > 0
    }

    /// Arms or disarms `track`. A disarmed track lets go of the keys held down, at `frame` of
    /// the block; a newly armed one hears the next key pressed, not the ones already down.
    pub(crate) fn set_armed(&mut self, chains: &mut [Chain], track: TrackId, armed: bool, frame: usize) {
        let index = self.armed().iter().position(|&t| t == track);
        match (armed, index) {
            (true, None) => {
                if let Some(slot) = self.armed.get_mut(self.armed_count) {
                    *slot = track;
                    self.armed_count += 1;
                }
            }
            (false, Some(index)) => {
                let last = self.armed_count - 1;
                self.armed.swap(index, last);
                self.armed_count = last;
                for chain in chains.iter_mut().filter(|c| c.instrument && c.track == track) {
                    for key in 0..=127u8 {
                        if sounding(&self.held, &self.sustained, key) {
                            chain.release_key(frame, key);
                        }
                    }
                }
            }
            _ => {}
        }
    }

    /// Plays what the controller sent since the last block, at the block's first frame, on the
    /// instrument chains of the armed tracks. Call it once the chains have begun the block.
    pub(crate) fn receive(&mut self, chains: &mut [Chain], position: u64) {
        let Some(consumer) = self.consumer.as_mut() else { return };
        // Bounded: what arrives while this runs waits for the next block.
        for _ in 0..LIVE_QUEUE {
            let Ok(event) = consumer.pop() else { break };
            let armed = self.armed.get(..self.armed_count).unwrap_or_default();
            let targets = chains.iter_mut().filter(|c| c.instrument && armed.contains(&c.track));
            match event {
                LiveEvent::NoteOn { key, velocity } => {
                    let Some(held) = self.held.get_mut(usize::from(key)) else { continue };
                    *held = true;
                    if let Some(sustained) = self.sustained.get_mut(usize::from(key)) {
                        *sustained = false;
                    }
                    let velocity = if velocity.is_finite() { velocity } else { 1.0 };
                    for chain in targets {
                        chain.note_on(0, position, key, velocity, UNTIL_RELEASED);
                    }
                }
                LiveEvent::NoteOff { key } => {
                    let Some(held) = self.held.get_mut(usize::from(key)) else { continue };
                    if !*held {
                        continue;
                    }
                    *held = false;
                    if self.sustain {
                        if let Some(sustained) = self.sustained.get_mut(usize::from(key)) {
                            *sustained = true;
                        }
                        continue;
                    }
                    for chain in targets {
                        chain.release_key(0, key);
                    }
                }
                LiveEvent::Sustain { on } => {
                    self.sustain = on;
                    if on {
                        continue;
                    }
                    for chain in targets {
                        for key in 0..=127u8 {
                            if self.sustained.get(usize::from(key)).copied().unwrap_or(false) {
                                chain.release_key(0, key);
                            }
                        }
                    }
                    self.sustained = [false; 128];
                }
                LiveEvent::AllNotesOff => {
                    for chain in targets {
                        for key in 0..=127u8 {
                            if sounding(&self.held, &self.sustained, key) {
                                chain.release_key(0, key);
                            }
                        }
                    }
                    self.held = [false; 128];
                    self.sustained = [false; 128];
                    self.sustain = false;
                }
            }
        }
    }
}

/// `key` sounds because of the controller: held down, or sustained by the pedal.
fn sounding(held: &[bool; 128], sustained: &[bool; 128], key: u8) -> bool {
    let at = usize::from(key);
    held.get(at).copied().unwrap_or(false) || sustained.get(at).copied().unwrap_or(false)
}
