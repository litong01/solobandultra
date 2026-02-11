//! Lyrics rendering and OSMD-inspired lyrics spacing.

use crate::model::*;
use super::constants::*;
use super::svg_builder::SvgBuilder;
use super::beat_map::compute_note_beat_times;

// ── Lyrics constants ────────────────────────────────────────────────

pub(super) const LYRICS_COLOR: &str = "#333333";
pub(super) const LYRICS_FONT_SIZE: f64 = 13.0;
pub(super) const LYRICS_PAD_BELOW: f64 = 16.0;
pub(super) const LYRICS_LINE_HEIGHT: f64 = 16.0;
pub(super) const LYRICS_MIN_Y_BELOW_STAFF: f64 = 54.0;
pub(super) const DIRECTION_WORDS_HEIGHT: f64 = 18.0;
pub(super) const DIRECTION_WORDS_LINE_HEIGHT: f64 = 15.0;

// ── OSMD-inspired lyrics spacing constants ──────────────────────────

const LYRICS_CHAR_WIDTH_FACTOR: f64 = 0.55;
pub(super) const LYRICS_MIN_GAP: f64 = 6.0;
pub(super) const LYRICS_DASH_EXTRA_GAP: f64 = 8.0;
pub(super) const LYRICS_OVERLAP_INTO_NEXT_MEASURE: f64 = 20.0;
pub(super) const MAX_LYRICS_ELONGATION_FACTOR: f64 = 2.5;

// ── Text width helpers ──────────────────────────────────────────────

/// Estimate the rendered width of a text string in pixels for a given font size.
pub(super) fn estimate_text_width(text: &str, font_size: f64) -> f64 {
    text.len() as f64 * font_size * LYRICS_CHAR_WIDTH_FACTOR
}

// ── LyricEvent ──────────────────────────────────────────────────────

/// Metadata about a lyric event at a specific beat.
#[derive(Clone, Debug)]
pub(super) struct LyricEvent {
    pub(super) beat_time: f64,
    pub(super) text_width: f64,
    /// True if this syllable is "begin" or "middle" (dash follows).
    pub(super) has_dash: bool,
    /// True if this is the last lyric-bearing beat in the measure.
    pub(super) is_last: bool,
}

/// Collect per-beat lyric events for a single measure across all parts.
pub(super) fn collect_lyric_events(parts: &[Part], mi: usize, divisions_map: &[i32]) -> Vec<LyricEvent> {
    let mut events: Vec<LyricEvent> = Vec::new();

    for (pidx, part) in parts.iter().enumerate() {
        if mi >= part.measures.len() { continue; }
        let measure = &part.measures[mi];
        let divisions = divisions_map[pidx].max(1);
        let beat_times = compute_note_beat_times(&measure.notes, divisions);

        for (i, note) in measure.notes.iter().enumerate() {
            if note.lyrics.is_empty() { continue; }
            let w = note.lyrics.iter()
                .map(|l| estimate_text_width(&l.text, LYRICS_FONT_SIZE))
                .fold(0.0f64, f64::max);
            if w <= 0.0 { continue; }
            let has_dash = note.lyrics.iter().any(|l| {
                matches!(l.syllabic.as_deref(), Some("begin") | Some("middle"))
            });
            events.push(LyricEvent {
                beat_time: beat_times[i],
                text_width: w,
                has_dash,
                is_last: false,
            });
        }
    }

    if events.is_empty() { return vec![]; }

    events.sort_by(|a, b| a.beat_time.partial_cmp(&b.beat_time).unwrap());
    let mut merged: Vec<LyricEvent> = Vec::new();
    for ev in &events {
        if let Some(last) = merged.last_mut() {
            if (last.beat_time - ev.beat_time).abs() < 0.001 {
                last.text_width = last.text_width.max(ev.text_width);
                last.has_dash = last.has_dash || ev.has_dash;
                continue;
            }
        }
        merged.push(ev.clone());
    }

    if let Some(last) = merged.last_mut() {
        last.is_last = true;
    }

    merged
}

/// Compute the minimum spacing required between two consecutive lyric events.
pub(super) fn lyric_pair_min_spacing(left: &LyricEvent, right: &LyricEvent) -> f64 {
    let gap = if left.has_dash {
        LYRICS_MIN_GAP + LYRICS_DASH_EXTRA_GAP
    } else {
        LYRICS_MIN_GAP
    };
    let right_half = if right.is_last {
        (right.text_width / 2.0 - LYRICS_OVERLAP_INTO_NEXT_MEASURE).max(0.0)
    } else {
        right.text_width / 2.0
    };
    left.text_width / 2.0 + gap + right_half
}

/// Compute the minimum measure width needed to accommodate lyrics.
pub(super) fn lyrics_min_measure_width(
    parts: &[Part],
    mi: usize,
    divisions_map: &[i32],
    beat_based_width: f64,
) -> f64 {
    let events = collect_lyric_events(parts, mi, divisions_map);
    if events.is_empty() { return 0.0; }

    let mut total = 0.0;
    for i in 0..events.len() {
        if i == 0 {
            total += events[i].text_width / 2.0;
        }
        if i > 0 {
            total += lyric_pair_min_spacing(&events[i - 1], &events[i]);
        }
    }
    if let Some(last) = events.last() {
        let right_half = if last.is_last {
            (last.text_width / 2.0 - LYRICS_OVERLAP_INTO_NEXT_MEASURE).max(0.0)
        } else {
            last.text_width / 2.0
        };
        total += right_half;
    }
    total += 28.0;

    let cap = beat_based_width * MAX_LYRICS_ELONGATION_FACTOR;
    total.min(cap)
}

/// Compute the lowest y-coordinate of rendered notes/stems in a measure.
pub(super) fn measure_lowest_note_y(
    measure: &Measure,
    staff_y: f64,
    clef: Option<&Clef>,
    transpose_octave: i32,
    staff_filter: Option<i32>,
) -> f64 {
    use super::beat_map::pitch_to_staff_y;

    let mut lowest = staff_y + STAFF_HEIGHT;
    for note in &measure.notes {
        if let Some(sf) = staff_filter {
            if note.staff.unwrap_or(1) != sf { continue; }
        }
        if let Some(ref pitch) = note.pitch {
            let note_y = staff_y + pitch_to_staff_y(pitch, clef, transpose_octave);
            let bottom = note_y + NOTEHEAD_RY;
            if bottom > lowest { lowest = bottom; }
            if note.stem.as_deref() == Some("down") {
                let stem_bottom = note_y + STEM_LENGTH;
                if stem_bottom > lowest { lowest = stem_bottom; }
            }
        }
    }
    lowest
}

pub(super) fn render_lyrics(
    svg: &mut SvgBuilder,
    measure: &Measure,
    note_positions: &[f64],
    lyrics_base_y: f64,
    staff_filter: Option<i32>,
) {
    for (i, note) in measure.notes.iter().enumerate() {
        if let Some(sf) = staff_filter {
            if note.staff.unwrap_or(1) != sf { continue; }
        }
        if i >= note_positions.len() { break; }
        let nx = note_positions[i];

        for lyric in &note.lyrics {
            let verse_offset = (lyric.number - 1) as f64 * LYRICS_LINE_HEIGHT;
            let ly = lyrics_base_y + verse_offset;

            let display_text = match lyric.syllabic.as_deref() {
                Some("begin") | Some("middle") => format!("{} -", lyric.text),
                _ => lyric.text.clone(),
            };

            svg.text(
                nx, ly,
                &display_text,
                LYRICS_FONT_SIZE,
                "normal",
                LYRICS_COLOR,
                "middle",
            );
        }
    }
}
