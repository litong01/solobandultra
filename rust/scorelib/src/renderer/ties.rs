//! Tie rendering (curved line between same-pitch noteheads across a tie chain).
//!
//! Ties look similar to slurs but connect two noteheads of the same pitch,
//! indicating sustained sound.  They are generally tighter/shorter than slurs
//! because they always span adjacent same-pitch notes.

use crate::model::*;
use super::svg_builder::SvgBuilder;
use super::beat_map::{pitch_to_staff_y, note_x_positions_from_beat_map};

const TIE_COLOR: &str = "#1a1a1a";
const TIE_NOTEHEAD_Y_OFFSET: f64 = 3.0;
const TIE_ENDPOINT_THICKNESS: f64 = 0.5;
const TIE_MID_THICKNESS: f64 = 1.5;
const TIE_HEIGHT_FACTOR: f64 = 0.18;
const TIE_MIN_HEIGHT: f64 = 4.0;
const TIE_MAX_HEIGHT: f64 = 15.0;

/// Recorded position of a tie-start event, keyed by pitch.
#[derive(Clone, Debug)]
pub(super) struct TieStart {
    pub(super) x: f64,
    pub(super) y: f64,
    pub(super) stem_up: bool,
    pub(super) staff_y: f64,
}

/// Build a string key for matching tied notes by pitch.
/// Two notes must share step, alter, and octave to be tied.
fn pitch_key(pitch: &Pitch) -> String {
    format!("{}{}{}", pitch.step, pitch.alter.unwrap_or(0.0), pitch.octave)
}

/// Collect tie positions for all notes in a single measure/staff and
/// render completed ties.  Open ties carry across measures via the map.
pub(super) fn collect_and_render_ties_for_measure(
    svg: &mut SvgBuilder,
    measure: &Measure,
    staff_y: f64,
    clef: Option<&Clef>,
    divisions: i32,
    transpose_octave: i32,
    staff_filter: Option<i32>,
    beat_x_map: &[(f64, f64)],
    open_ties: &mut std::collections::HashMap<String, TieStart>,
) {
    if measure.notes.is_empty() {
        return;
    }

    let note_positions = note_x_positions_from_beat_map(&measure.notes, divisions, beat_x_map);

    for (i, note) in measure.notes.iter().enumerate() {
        if let Some(sf) = staff_filter {
            if note.staff.unwrap_or(1) != sf { continue; }
        }
        if note.rest || note.grace { continue; }
        if !note.tie_start && !note.tie_stop { continue; }

        let pitch = match note.pitch {
            Some(ref p) => p,
            None => continue,
        };

        let nx = note_positions[i];
        let note_y = staff_y + pitch_to_staff_y(pitch, clef, transpose_octave);
        let stem_up = match note.stem.as_deref() {
            Some("up") => true,
            Some("down") => false,
            _ => note_y >= staff_y + 20.0,
        };

        let key = pitch_key(pitch);

        // Process tie_stop FIRST — the same note can be both stop and start
        // (middle of a tie chain: tie_stop + tie_start).
        if note.tie_stop {
            if let Some(start) = open_ties.remove(&key) {
                render_tie(svg, &start, nx, note_y, stem_up);
            }
        }

        if note.tie_start {
            open_ties.insert(key, TieStart {
                x: nx,
                y: note_y,
                stem_up,
                staff_y,
            });
        }
    }
}

/// Draw a tie curve between two note positions.
fn render_tie(
    svg: &mut SvgBuilder,
    start: &TieStart,
    end_x: f64,
    end_y: f64,
    _end_stem_up: bool,
) {
    // Tie curves opposite to the stem direction (same convention as slurs).
    let above = !start.stem_up;
    let y_dir = if above { -1.0 } else { 1.0 };

    let sx = start.x;
    let sy = start.y + y_dir * TIE_NOTEHEAD_Y_OFFSET;
    let ex = end_x;
    let ey = end_y + y_dir * TIE_NOTEHEAD_Y_OFFSET;

    let dx = (ex - sx).abs().max(1.0);
    let height = (dx * TIE_HEIGHT_FACTOR).clamp(TIE_MIN_HEIGHT, TIE_MAX_HEIGHT);
    let mid_y = (sy + ey) / 2.0;

    let cp1x = sx + dx * 0.25;
    let cp1y = mid_y + y_dir * height;
    let cp2x = sx + dx * 0.75;
    let cp2y = mid_y + y_dir * height;

    let ep_off = TIE_ENDPOINT_THICKNESS * y_dir;
    let cp_off = TIE_MID_THICKNESS * y_dir;

    let path = format!(
        "M{:.1},{:.1} C{:.1},{:.1} {:.1},{:.1} {:.1},{:.1} L{:.1},{:.1} C{:.1},{:.1} {:.1},{:.1} {:.1},{:.1} Z",
        sx, sy,
        cp1x, cp1y,
        cp2x, cp2y,
        ex, ey,
        ex, ey + ep_off,
        cp2x, cp2y + cp_off,
        cp1x, cp1y + cp_off,
        sx, sy + ep_off,
    );

    svg.path(&path, TIE_COLOR, "none", 0.0);
}

/// Draw continuation ties for any still-open ties at the end of a system.
pub(super) fn render_open_tie_continuations(
    svg: &mut SvgBuilder,
    open_ties: &std::collections::HashMap<String, TieStart>,
    system_x_end: f64,
) {
    for (_key, start) in open_ties.iter() {
        let end_y = start.y;
        render_tie(svg, start, system_x_end, end_y, start.stem_up);
    }
}
