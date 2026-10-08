//! Deterministic timing and velocity humanization for accompaniment tracks.
//!
//! The same score and options always produce the same offsets: jitter is drawn
//! from a fingerprint of the chord sequence, the timemap, and the energy
//! level, never from a fresh random source. The melody and metronome are not
//! passed through this module. Swing is optional and stays off unless the
//! accompaniment style asks for it.

use std::collections::{BTreeMap, VecDeque};

use crate::midi::{ticks_to_ms, ms_to_ticks, MidiEvent};
use crate::timemap::{felt_beats, TimemapEntry};

/// Which accompaniment part an event list belongs to.
#[derive(Clone, Copy, Debug)]
pub(crate) enum Role {
    Piano,
    Bass,
    Strings,
    Drums,
}

/// Long fraction of a swung eighth pair. The offbeat lands at 58% of the beat
/// (a 58/42 split) instead of halfway.
const SWING_BEAT_FRACTION: f64 = 0.58;

const SALT_TIME: u64 = 0x5449_4D45;
const SALT_VEL: u64 = 0x5645_4C21;

/// FNV-1a fingerprint of the musical input that drives accompaniment.
pub(crate) struct Fingerprint {
    state: u64,
}

impl Fingerprint {
    pub(crate) fn new() -> Self {
        Self {
            state: 0xcbf29ce484222325,
        }
    }

    pub(crate) fn u8(&mut self, value: u8) {
        self.bytes(&[value]);
    }

    pub(crate) fn i64(&mut self, value: i64) {
        self.bytes(&value.to_le_bytes());
    }

    pub(crate) fn finish(&self) -> u64 {
        self.state
    }

    fn bytes(&mut self, bytes: &[u8]) {
        for &byte in bytes {
            self.state ^= byte as u64;
            self.state = self.state.wrapping_mul(0x100000001b3);
        }
    }
}

#[derive(Clone, Copy)]
enum BeatKind {
    Downbeat,
    Backbeat,
    OtherBeat,
    Offbeat,
}

struct Placement {
    kind: BeatKind,
    beat_dur_ms: f64,
}

#[derive(Clone)]
struct NotePair {
    on_idx: usize,
    off_idx: usize,
    channel: u8,
    note: u8,
    velocity: u8,
}

/// Shift note times and reshape velocities in place.
///
/// Note-on and note-off of a pair move by the same amount, so duration is
/// preserved. When `swing` is set, offbeat eighths land on a 58/42 grid.
/// Straight styles leave those eighths on the beat and only apply the small
/// role jitter. Bass and kick sit late, hi-hats sit early.
pub(crate) fn humanize(
    events: &mut [MidiEvent],
    role: Role,
    seed: u64,
    timemap: &[TimemapEntry],
    swing: bool,
) {
    if timemap.is_empty() || events.is_empty() {
        return;
    }

    let pairs = pair_notes(events);
    let mut planned: Vec<(usize, usize, u32, u32, u8)> = Vec::with_capacity(pairs.len());

    for pair in &pairs {
        let onset_ms = ticks_to_ms(events[pair.on_idx].tick, timemap);
        let release_ms = ticks_to_ms(events[pair.off_idx].tick, timemap);
        let delta = timing_delta_ms(
            onset_ms,
            role,
            pair.note,
            events[pair.on_idx].tick,
            seed,
            timemap,
            swing,
        );
        let clamped = if onset_ms + delta < 0.0 {
            -onset_ms
        } else {
            delta
        };
        let new_on_ms = onset_ms + clamped;
        let new_off_ms = (release_ms + clamped).max(new_on_ms);
        let new_on = ms_to_ticks(new_on_ms, timemap);
        let mut new_off = ms_to_ticks(new_off_ms, timemap);
        if new_off <= new_on {
            new_off = new_on.saturating_add(1);
        }
        let new_vel = shape_velocity(
            pair.velocity,
            onset_ms,
            role,
            pair.note,
            events[pair.on_idx].tick,
            seed,
            timemap,
        );
        planned.push((pair.on_idx, pair.off_idx, new_on, new_off, new_vel));
    }

    for (on_idx, off_idx, on_tick, off_tick, vel) in planned {
        events[on_idx].tick = on_tick;
        if events[on_idx].bytes.len() >= 3 {
            events[on_idx].bytes[2] = vel;
        }
        events[off_idx].tick = off_tick;
    }

    separate_same_pitch(events);
}

fn pair_notes(events: &[MidiEvent]) -> Vec<NotePair> {
    let mut pending: BTreeMap<(u8, u8), VecDeque<(usize, u8)>> = BTreeMap::new();
    let mut pairs = Vec::new();

    for (index, event) in events.iter().enumerate() {
        if event.bytes.len() < 3 {
            continue;
        }
        let status = event.bytes[0];
        let kind = status & 0xF0;
        let channel = status & 0x0F;
        let note = event.bytes[1];
        let velocity = event.bytes[2];
        if kind == 0x90 && velocity > 0 {
            pending
                .entry((channel, note))
                .or_default()
                .push_back((index, velocity));
        } else if kind == 0x80 || (kind == 0x90 && velocity == 0) {
            if let Some(queue) = pending.get_mut(&(channel, note)) {
                if let Some((on_idx, on_vel)) = queue.pop_front() {
                    pairs.push(NotePair {
                        on_idx,
                        off_idx: index,
                        channel,
                        note,
                        velocity: on_vel,
                    });
                }
            }
        }
    }

    pairs
}

/// Pull a note-off back so it cannot cancel the next attack of the same pitch.
fn separate_same_pitch(events: &mut [MidiEvent]) {
    let pairs = pair_notes(events);
    let mut groups: BTreeMap<(u8, u8), Vec<NotePair>> = BTreeMap::new();
    for pair in pairs {
        groups.entry((pair.channel, pair.note)).or_default().push(pair);
    }

    for list in groups.values() {
        let mut ordered = list.clone();
        ordered.sort_by_key(|pair| events[pair.on_idx].tick);
        for window in ordered.windows(2) {
            let on = events[window[0].on_idx].tick;
            let off = events[window[0].off_idx].tick;
            let next_on = events[window[1].on_idx].tick;
            if off >= next_on && next_on > on {
                events[window[0].off_idx].tick = next_on - 1;
            }
        }
    }
}

fn timing_delta_ms(
    onset_ms: f64,
    role: Role,
    note: u8,
    tick: u32,
    seed: u64,
    timemap: &[TimemapEntry],
    swing: bool,
) -> f64 {
    let (lo, hi) = timing_bounds(role, note);
    let jitter = pick_i32(seed, SALT_TIME, tick, note, role, lo, hi) as f64;
    let swing_ms = if swing {
        placement_at(onset_ms, timemap)
            .filter(|place| matches!(place.kind, BeatKind::Offbeat))
            .map(|place| (SWING_BEAT_FRACTION - 0.5) * place.beat_dur_ms)
            .unwrap_or(0.0)
    } else {
        0.0
    };
    swing_ms + jitter
}

fn timing_bounds(role: Role, note: u8) -> (i32, i32) {
    match role {
        Role::Piano => (-12, 12),
        Role::Bass => (6, 14),
        Role::Strings => (2, 8),
        Role::Drums => match note {
            42 | 44 | 46 => (-14, -5),
            36 | 35 => (4, 12),
            _ => (-4, 10),
        },
    }
}

fn shape_velocity(
    velocity: u8,
    onset_ms: f64,
    role: Role,
    note: u8,
    tick: u32,
    seed: u64,
    timemap: &[TimemapEntry],
) -> u8 {
    let accent = placement_at(onset_ms, timemap)
        .map(|place| match place.kind {
            BeatKind::Downbeat => 1.12,
            BeatKind::Backbeat => 1.06,
            BeatKind::OtherBeat => 1.0,
            BeatKind::Offbeat => 0.84,
        })
        .unwrap_or(1.0);
    let jitter = pick_i32(seed, SALT_VEL, tick, note, role, -3, 3) as f64;
    (velocity as f64 * accent + jitter).round().clamp(1.0, 127.0) as u8
}

fn placement_at(ms: f64, timemap: &[TimemapEntry]) -> Option<Placement> {
    let entry = timemap.iter().find(|entry| {
        ms >= entry.timestamp_ms && ms < entry.timestamp_ms + entry.duration_ms
    })?;
    let beats = felt_beats(entry.time_sig.0, entry.time_sig.1).max(1);
    let beat_dur = entry.duration_ms / beats as f64;
    if beat_dur <= 1.0 {
        return None;
    }
    let eighth = beat_dur / 2.0;
    let rel = ms - entry.timestamp_ms;
    let eighth_index = (rel / eighth).round() as i32;
    let nearest = eighth_index as f64 * eighth;
    if (rel - nearest).abs() > eighth * 0.35 {
        return None;
    }
    // A tick that quantizes onto the next barline is that bar's downbeat.
    if eighth_index >= beats * 2 {
        return Some(Placement {
            kind: BeatKind::Downbeat,
            beat_dur_ms: beat_dur,
        });
    }
    let eighth_index = eighth_index.max(0);
    let on_beat = eighth_index % 2 == 0;
    let beat_index = eighth_index / 2;
    let kind = if !on_beat {
        BeatKind::Offbeat
    } else if beat_index == 0 {
        BeatKind::Downbeat
    } else if beat_index % 2 == 1 {
        BeatKind::Backbeat
    } else {
        BeatKind::OtherBeat
    };
    Some(Placement {
        kind,
        beat_dur_ms: beat_dur,
    })
}

fn pick_i32(seed: u64, salt: u64, tick: u32, note: u8, role: Role, lo: i32, hi: i32) -> i32 {
    let span = (hi - lo + 1).max(1) as u64;
    let mut hash = seed ^ 0x9E3779B97F4A7C15;
    hash = hash.wrapping_mul(0x100000001b3) ^ salt;
    hash = hash.wrapping_mul(0x100000001b3) ^ tick as u64;
    hash = hash.wrapping_mul(0x100000001b3) ^ note as u64;
    hash = hash.wrapping_mul(0x100000001b3) ^ role_tag(role) as u64;
    lo + (hash % span) as i32
}

fn role_tag(role: Role) -> u8 {
    match role {
        Role::Piano => 1,
        Role::Bass => 2,
        Role::Strings => 3,
        Role::Drums => 4,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn bar() -> Vec<TimemapEntry> {
        vec![TimemapEntry {
            index: 0,
            original_index: 0,
            timestamp_ms: 0.0,
            duration_ms: 2000.0,
            tempo_bpm: 120.0,
            time_sig: (4, 4),
            divisions: 1,
            effective_quarters: 4.0,
        }]
    }

    fn note_at(channel: u8, note: u8, start_ms: f64, dur_ms: f64, vel: u8, timemap: &[TimemapEntry]) -> Vec<MidiEvent> {
        let on = ms_to_ticks(start_ms, timemap);
        let off = ms_to_ticks(start_ms + dur_ms, timemap).max(on + 1);
        vec![
            MidiEvent {
                tick: on,
                bytes: vec![0x90 | channel, note, vel],
            },
            MidiEvent {
                tick: off,
                bytes: vec![0x80 | channel, note, 0],
            },
        ]
    }

    #[test]
    fn humanize_is_deterministic() {
        let timemap = bar();
        let mut once = note_at(1, 60, 250.0, 200.0, 80, &timemap);
        let mut twice = once.clone();
        humanize(&mut once, Role::Piano, 0xABC, &timemap, true);
        humanize(&mut twice, Role::Piano, 0xABC, &timemap, true);
        assert_eq!(once, twice);
    }

    #[test]
    fn different_seeds_can_move_a_note() {
        let timemap = bar();
        let mut a = Vec::new();
        let mut b = Vec::new();
        for step in 0..8 {
            let start = 500.0 + step as f64 * 10.0;
            a.extend(note_at(2, 36, start, 150.0, 90, &timemap));
            b.extend(note_at(2, 36, start, 150.0, 90, &timemap));
        }
        humanize(&mut a, Role::Bass, 1, &timemap, true);
        humanize(&mut b, Role::Bass, 2, &timemap, true);
        assert_ne!(a, b);
    }

    #[test]
    fn bass_lays_back_and_offbeat_swings() {
        let timemap = bar();
        let mut bass = note_at(2, 36, 500.0, 180.0, 90, &timemap);
        humanize(&mut bass, Role::Bass, 7, &timemap, true);
        let moved = ticks_to_ms(bass[0].tick, &timemap);
        assert!(
            (506.0..=515.5).contains(&moved),
            "bass at 500ms should land 6–14ms late, got {moved}"
        );

        let mut hat = note_at(9, 42, 500.0, 80.0, 55, &timemap);
        humanize(&mut hat, Role::Drums, 7, &timemap, true);
        let hat_ms = ticks_to_ms(hat[0].tick, &timemap);
        assert!(
            (485.0..=496.0).contains(&hat_ms),
            "on-beat hat should be early, got {hat_ms}"
        );

        let straight = note_at(1, 64, 250.0, 180.0, 80, &timemap);
        let mut piano = straight.clone();
        humanize(&mut piano, Role::Piano, 7, &timemap, true);
        let swung = ticks_to_ms(piano[0].tick, &timemap);
        // 40ms swing (0.08 * 500) plus piano jitter of ±12ms.
        assert!(
            (276.0..=304.0).contains(&swung),
            "offbeat eighth should swing late, got {swung}"
        );
        let dur_before = ticks_to_ms(straight[1].tick, &timemap) - 250.0;
        let dur_after = ticks_to_ms(piano[1].tick, &timemap) - swung;
        assert!(
            (dur_after - dur_before).abs() < 2.0,
            "swing should keep the note length"
        );
    }

    #[test]
    fn downbeat_is_louder_than_offbeat() {
        let timemap = bar();
        let mut events = note_at(1, 60, 0.0, 200.0, 80, &timemap);
        events.extend(note_at(1, 64, 250.0, 200.0, 80, &timemap));
        humanize(&mut events, Role::Piano, 11, &timemap, true);
        let down = events[0].bytes[2] as i32;
        let off = events[2].bytes[2] as i32;
        assert!(
            down > off,
            "downbeat vel {down} should exceed offbeat vel {off}"
        );
    }

    #[test]
    fn straight_styles_keep_the_offbeat_on_the_grid() {
        let timemap = bar();
        let mut piano = note_at(1, 64, 250.0, 180.0, 80, &timemap);
        humanize(&mut piano, Role::Piano, 7, &timemap, false);
        let moved = ticks_to_ms(piano[0].tick, &timemap);
        assert!(
            (238.0..=262.0).contains(&moved),
            "straight offbeat should stay within piano jitter of 250ms, got {moved}"
        );
    }
}
