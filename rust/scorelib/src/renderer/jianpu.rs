//! Jianpu (numbered musical notation) rendering.
//!
//! Conventions (key-based movable do):
//! - Digits 1–7 = scale degrees (do, re, mi, fa, sol, la, si). 0 = rest.
//! - Key sets which pitch is 1 (e.g. C major → C=1, G major → G=1).
//! - Dots above/below = octave (one dot above = one octave higher, etc.).
//! - Lines under the number = duration (one line = eighth, two = sixteenth; none = quarter).
//! - Dot after number = dotted (add half); dash after = lengthen (half, whole).
//! - # and b before the number for accidentals (e.g. #4, b7).

use crate::model::Pitch;
use super::beat_map::note_x_positions_from_beat_map;
use super::constants::*;
use super::svg_builder::SvgBuilder;

/// Default font size for Jianpu digits.
const JIANPU_FONT_SIZE: f64 = 22.0;
/// Vertical spacing for octave dots (above/below the digit).
const JIANPU_DOT_OFFSET: f64 = 10.0;
/// Radius of octave dots.
const JIANPU_DOT_R: f64 = 2.0;
/// Vertical offset for duration underlines below the digit baseline.
const JIANPU_UNDERLINE_OFFSET: f64 = 6.0;
/// Spacing between multiple underlines.
const JIANPU_UNDERLINE_GAP: f64 = 3.0;
/// Width of one digit cell for underlines (approximate).
const JIANPU_DIGIT_WIDTH: f64 = 14.0;
/// Horizontal offset for dash after number (half/whole).
const JIANPU_DASH_OFFSET: f64 = 4.0;
const JIANPU_DASH_WIDTH: f64 = 8.0;
/// Accidental offset (left of digit).
const JIANPU_ACCIDENTAL_OFFSET: f64 = 6.0;
const JIANPU_ACCIDENTAL_SIZE: f64 = 14.0;

/// Key fifths (e.g. 0 = C, 1 = G) to root pitch class in semitones (0–11).
#[inline]
fn key_root_semitone(fifths: i32) -> i32 {
    (7 * fifths).rem_euclid(12)
}

/// Map semitone offset from root (0–11) to (degree 1–7, accidental).
/// Major scale: 0→1, 2→2, 4→3, 5→4, 7→5, 9→6, 11→7.
/// Chromatic: 1→#1, 3→#2, 6→#3 or b4 (use #3), 8→#4, 10→#5, (11→7 or b7; use b7 for leading tone).
fn semitone_to_degree_acc(semi: i32) -> (u8, Option<&'static str>) {
    let s = semi.rem_euclid(12);
    match s {
        0 => (1, None),
        1 => (1, Some("#")),
        2 => (2, None),
        3 => (2, Some("#")),
        4 => (3, None),
        5 => (4, None),
        6 => (4, Some("#")), // or b5; #4 is common
        7 => (5, None),
        8 => (5, Some("#")),
        9 => (6, None),
        10 => (6, Some("#")),
        11 => (7, Some("b")), // leading tone
        _ => (1, None),
    }
}

/// Convert pitch and key to Jianpu digit (1–7), octave dots (positive = above, negative = below), and accidental.
/// Middle octave (C4–B4, MIDI 60–71) = 0 dots.
pub(super) fn pitch_to_jianpu(
    pitch: &Pitch,
    key_fifths: i32,
) -> (u8, i32, Option<&'static str>) {
    let midi = pitch.to_midi();
    let root = key_root_semitone(key_fifths);
    let pitch_class = midi.rem_euclid(12);
    let semi = (pitch_class - root).rem_euclid(12);
    let (degree, accidental) = semitone_to_degree_acc(semi);

    // Octave: C4 = 60. Middle = 0 dots. Each octave up = +1 dot above, each down = -1 dot below.
    let octave_rel = (midi / 12) - 5; // 60/12 - 5 = 0
    let octave_dots = octave_rel.clamp(-2, 2); // limit to 2 dots each way for readability

    (degree, octave_dots, accidental)
}

/// Duration in quarter notes to Jianpu duration style: underlines (0–2), suffix dot, suffix dashes.
/// Quarter = 0 underlines; eighth = 1; sixteenth = 2. Dotted = add dot after. Half = 1 dash, whole = 2 dashes.
pub(super) fn duration_to_jianpu(
    duration_quarters: f64,
    dot: bool,
) -> (u8, bool, u8) {
    let underlines = if duration_quarters <= 0.26 {
        2 // 16th or shorter
    } else if duration_quarters <= 0.51 {
        1 // eighth
    } else {
        0 // quarter or longer
    };
    let suffix_dashes = if duration_quarters >= 3.99 {
        2 // whole
    } else if duration_quarters >= 1.99 {
        1 // half
    } else {
        0
    };
    (underlines, dot, suffix_dashes)
}

/// Draw a single Jianpu note/rest at (x, y). y is the baseline for the digit.
/// For rest we draw "0". For notes we draw digit, octave dots, accidental, underlines, suffix dot/dash.
pub(super) fn render_jianpu_note(
    svg: &mut SvgBuilder,
    x: f64,
    y: f64,
    digit: u8,
    octave_dots: i32,
    accidental: Option<&str>,
    underlines: u8,
    suffix_dot: bool,
    suffix_dashes: u8,
) {
    let fill = NOTE_COLOR;
    let mut draw_x = x;

    if let Some(acc) = accidental {
        svg.text(
            draw_x - JIANPU_ACCIDENTAL_OFFSET,
            y,
            acc,
            JIANPU_ACCIDENTAL_SIZE,
            "normal",
            fill,
            "end",
        );
        draw_x += 2.0;
    }

    let s: String = if digit == 0 { "0".into() } else { ((b'0' + digit) as char).to_string() };
    svg.text(
        draw_x,
        y,
        &s,
        JIANPU_FONT_SIZE,
        "normal",
        fill,
        "middle",
    );

    // Octave dots above
    for i in 0..octave_dots {
        let dy = -JIANPU_DOT_OFFSET - (i as f64 * (JIANPU_DOT_OFFSET * 0.6));
        svg.circle(draw_x + 4.0, y + dy, JIANPU_DOT_R, fill);
    }
    // Octave dots below
    for i in 0..(-octave_dots).max(0) {
        let dy = JIANPU_DOT_OFFSET + (i as f64 * (JIANPU_DOT_OFFSET * 0.6));
        svg.circle(draw_x + 4.0, y + dy, JIANPU_DOT_R, fill);
    }

    // Duration underlines
    let ul_y = y + JIANPU_UNDERLINE_OFFSET;
    for i in 0..underlines {
        let uy = ul_y + (i as f64 * JIANPU_UNDERLINE_GAP);
        svg.line(
            draw_x - 2.0,
            uy,
            draw_x + JIANPU_DIGIT_WIDTH,
            uy,
            fill,
            1.2,
        );
    }

    // Suffix dot (dotted note)
    if suffix_dot {
        svg.circle(draw_x + JIANPU_DIGIT_WIDTH + 4.0, y - 2.0, 2.0, fill);
    }

    // Suffix dashes (half/whole)
    let mut dash_x = draw_x + JIANPU_DIGIT_WIDTH + if suffix_dot { 10.0 } else { JIANPU_DASH_OFFSET };
    for _ in 0..suffix_dashes {
        svg.line(dash_x, y, dash_x + JIANPU_DASH_WIDTH, y, fill, 1.5);
        dash_x += JIANPU_DASH_WIDTH + 2.0;
    }
}

/// Render one measure of Jianpu for the given staff. Uses the same beat_x_map as staff rendering
/// so note positions align. Repeats/barlines/directions are drawn by the caller (same as staff).
pub(super) fn render_jianpu_measure(
    svg: &mut SvgBuilder,
    measure: &crate::model::Measure,
    staff_y: f64,
    staff_filter: i32,
    key_fifths: i32,
    divisions: i32,
    beat_x_map: &[(f64, f64)],
) {
    let note_positions = note_x_positions_from_beat_map(
        &measure.notes,
        divisions,
        beat_x_map,
    );
    let div = divisions.max(1) as f64;

    for (i, note) in measure.notes.iter().enumerate() {
        if note.staff.unwrap_or(1) != staff_filter {
            continue;
        }
        let nx = note_positions.get(i).copied().unwrap_or(0.0);

        if note.rest {
            let duration_quarters = note.duration as f64 / div * 4.0;
            let (underlines, dot, dashes) = duration_to_jianpu(duration_quarters, note.dot);
            render_jianpu_note(svg, nx, staff_y, 0, 0, None, underlines, dot, dashes);
            continue;
        }

        if note.grace {
            if let Some(ref pitch) = note.pitch {
                let (digit, octave_dots, accidental) = pitch_to_jianpu(pitch, key_fifths);
                render_jianpu_note(
                    svg, nx, staff_y,
                    digit, octave_dots, accidental,
                    1, false, 0, // grace: one underline
                );
            }
            continue;
        }

        if let Some(ref pitch) = note.pitch {
            let (digit, octave_dots, accidental) = pitch_to_jianpu(pitch, key_fifths);
            let duration_quarters = note.duration as f64 / div * 4.0;
            let (underlines, suffix_dot, suffix_dashes) = duration_to_jianpu(duration_quarters, note.dot);
            render_jianpu_note(
                svg, nx, staff_y,
                digit, octave_dots, accidental,
                underlines, suffix_dot, suffix_dashes,
            );
        }
    }
}
