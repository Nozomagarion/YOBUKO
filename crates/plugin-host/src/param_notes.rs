// Copyright (C) 2026 The Suhina contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! (id, value) pairs an audio thread hands the main thread between blocks: what a plug-in
//! changed itself (its own editor, its internal mapping), or what the engine sent with the
//! blocks (a panel set, an automation lane). Written into fixed slots in turn — a burst past
//! the room loses the oldest slot's value, so what a parameter is at now is what survives —
//! and read out again whole, in the order the writes were made. The audio thread never locks,
//! waits or allocates.

use std::sync::Arc;
use std::sync::atomic::{AtomicU32, AtomicU64, AtomicUsize, Ordering};

/// Most pairs one report holds: writes past it lose the oldest slot's value until that
/// parameter is written again. More parameter writes than this in the time between two
/// main-thread reads has not been seen; the values a plug-in runs with are re-read by the
/// listing every couple of seconds.
const SLOTS: usize = 256;

/// A slot with nothing written in it yet: not a parameter id (ids are 32-bit).
const FREE: u64 = u64::MAX;

/// One slot: a sequence number, odd while a write is under way, and the pair it holds.
struct Slot {
    seq: AtomicUsize,
    id: AtomicU64,
    value: AtomicU64,
}

impl Slot {
    const fn new() -> Self {
        Self { seq: AtomicUsize::new(0), id: AtomicU64::new(FREE), value: AtomicU64::new(0) }
    }
}

/// What the audio thread noted since the main thread read it last. Made before processing
/// starts; the audio thread only writes, the main thread only reads.
pub(crate) struct ParamNotes {
    slots: Box<[Slot]>,
    cursor: AtomicU32,
}

/// A report shared between an audio thread and the main thread.
pub(crate) type Notes = Arc<ParamNotes>;

/// The main thread's memory of what each slot said last, one entry per slot (the id as it was
/// written, and the value's bits): what [`ParamNotes::take`] reports a slot's change once,
/// not again on every read.
pub(crate) type Reads = Vec<Option<(u64, u64)>>;

impl ParamNotes {
    pub(crate) fn new() -> Self {
        Self { slots: (0..SLOTS).map(|_| Slot::new()).collect(), cursor: AtomicU32::new(0) }
    }

    /// [`Reads`] as empty as its slots are: no slot has said anything yet.
    pub(crate) fn fresh_reads() -> Reads {
        vec![None; SLOTS]
    }

    /// Notes `value` as parameter `id`'s value now. Real-time safe: no lock, no allocation,
    /// it never waits.
    pub(crate) fn note(&self, id: u32, value: f64) {
        let at = self.cursor.fetch_add(1, Ordering::Relaxed) as usize % self.slots.len();
        let Some(slot) = self.slots.get(at) else { return };
        // The sequence marks the write: odd while it is under way, even when the pair holds
        // together, so a read either sees it whole or sees none of it.
        let _ = slot.seq.fetch_add(1, Ordering::AcqRel);
        slot.id.store(u64::from(id), Ordering::Relaxed);
        slot.value.store(value.to_bits(), Ordering::Relaxed);
        let _ = slot.seq.fetch_add(1, Ordering::AcqRel);
    }

    /// What has been noted since `read` last saw the slots, in the order the writes were
    /// made, each pair a slot's latest; `read` is left holding what was read now. A slot
    /// still under a write is tried again, so a read sees a write whole or not at all, and
    /// what it missed this time comes with the next read. Never clears: what each slot said
    /// stays, so the audio thread can write over it without waiting for the reader.
    pub(crate) fn take(&self, read: &mut Reads) -> Vec<(u32, f64)> {
        let start = self.cursor.load(Ordering::Relaxed) as usize % self.slots.len();
        let mut taken = Vec::new();
        for step in 0..self.slots.len() {
            let at = (start + step) % self.slots.len();
            let Some(slot) = self.slots.get(at) else { continue };
            loop {
                let before = slot.seq.load(Ordering::Acquire);
                if before % 2 == 1 {
                    continue;
                }
                let id = slot.id.load(Ordering::Relaxed);
                let value = slot.value.load(Ordering::Relaxed);
                if slot.seq.load(Ordering::Acquire) != before {
                    continue;
                }
                if id != FREE {
                    // Reported once: what the caller last read from this slot (the value by
                    // its bits, so a value that only looks equal is still told).
                    if read[at] != Some((id, value)) {
                        taken.push((id as u32, f64::from_bits(value)));
                    }
                    read[at] = Some((id, value));
                }
                break;
            }
        }
        taken
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_noted_value_is_reported_once_and_a_change_again() {
        let notes = ParamNotes::new();
        let mut read = ParamNotes::fresh_reads();
        notes.note(5, 0.1);
        notes.note(7, 0.9);
        assert_eq!(notes.take(&mut read), [(5, 0.1), (7, 0.9)]);
        // Nothing changed: the same values are not reported again, and the reader of these
        // pairs takes what a later set says, not what a slot said once.
        assert!(notes.take(&mut read).is_empty());
        // The same value written again (through another slot) is reported again; the readers
        // of these pairs take it as the same value they already know.
        notes.note(5, 0.2);
        assert_eq!(notes.take(&mut read), [(5, 0.2)]);
    }

    #[test]
    fn a_burst_past_the_room_loses_the_oldest() {
        let notes = ParamNotes::new();
        let mut read = ParamNotes::fresh_reads();
        for id in 0..(SLOTS as u32 + 32) {
            notes.note(id, 0.5);
        }
        let taken = notes.take(&mut read);
        assert_eq!(taken.len(), SLOTS, "one slot each");
        assert_eq!(taken.first().map(|(id, _)| *id), Some(32), "the first 32 ids were written over");
        // The reads came in the order the writes were made.
        let ids: Vec<u32> = taken.iter().map(|(id, _)| *id).collect();
        let sorted: Vec<u32> = (32..(SLOTS as u32 + 32)).collect();
        assert_eq!(ids, sorted);
    }

    #[test]
    fn the_reads_are_in_the_order_the_writes_were_made() {
        let notes = ParamNotes::new();
        let mut read = ParamNotes::fresh_reads();
        // Enough writes to wrap the slots, with one parameter written first and last: its
        // last value is what the read ends with.
        for id in 0..(SLOTS as u32) {
            notes.note(id, 0.5);
        }
        notes.note(5, 0.9);
        let taken = notes.take(&mut read);
        assert_eq!(taken.iter().rev().find(|(id, _)| *id == 5).map(|(_, value)| *value), Some(0.9));
    }
}
