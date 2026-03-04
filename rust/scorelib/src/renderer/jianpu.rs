//! Jianpu (numbered musical notation) rendering.
//!
//! Conventions (key-based movable do), aligned with sample.rs reference:
//! - Digits 1–7 = scale degrees (do, re, mi, fa, sol, la, si). 0 = rest.
//! - Key sets which pitch is 1 (e.g. C major → C=1, G major → G=1). Mode (major/minor/etc.) changes the scale.
//! - Dots above/below = octave (one dot above = one octave higher, etc.).
//! - Underlines = duration beams (one line = eighth, two = sixteenth, three = 32nd); drawn as continuous spans per voice.
//! - Dot after number = dotted (add half); dash after = lengthen (half, whole).
//! - ^ and _ (and ^^, __) before the number for accidentals (sharp and flat).

use crate::model::Pitch;
use super::constants::*;
use super::svg_builder::SvgBuilder;

/// Default font size for Jianpu digits. Exposed so the lyrics path can shift by half note width when needed.
pub(super) const JIANPU_FONT_SIZE: f64 = 22.0;

/// One fixed width for every note head (0–7), excluding duration suffix (e.g. "." or " -").
/// In jianpu, all note heads are designed to occupy the same visual width; we use the font size.
pub(super) fn jianpu_note_head_width(font_size: f64) -> f64 {
    font_size
}

/// Vertical distance from digit baseline to the first octave dot (above or below).
const JIANPU_DOT_OFFSET: f64 = 20.0;
/// Vertical step between multiple octave dots (so they don’t overlap the number or each other).
const JIANPU_DOT_STEP: f64 = 10.0;
/// Key label font size (e.g. "1 = C").
const JIANPU_KEY_LABEL_FONT: f64 = 14.0;
/// Vertical spacing between stacked chord notes. Must be at least one digit height to avoid overlap
/// (digit is JIANPU_FONT_SIZE tall; add gap so octave dots don’t touch).
/// Half-height of one rendered note (digit half + octave dots; up to 2 dots above/below).
fn jianpu_half_note_height() -> f64 {
    JIANPU_FONT_SIZE / 2.0 + JIANPU_DOT_OFFSET + 2.0 * JIANPU_DOT_STEP
}
/// Extra gap between stacked chord notes (scales with font).
const JIANPU_CHORD_STACK_GAP_RATIO: f64 = 0.12;
/// Vertical spacing between stacked chord notes; derived from note height so it stays correct when font or dot metrics change.
fn jianpu_chord_stack_spacing() -> f64 {
    2.0 * jianpu_half_note_height() + JIANPU_CHORD_STACK_GAP_RATIO * JIANPU_FONT_SIZE
}

/// Key fifths (e.g. 0 = C, 1 = G) to root pitch class in semitones (0–11).
#[inline]
fn key_root_semitone(fifths: i32) -> i32 {
    (7 * fifths).rem_euclid(12)
}

/// Scale intervals for each mode (semitone steps from tonic). Same as sample.rs.
fn mode_intervals(mode: Option<&str>) -> [u8; 7] {
    match mode.map(|s| s.to_lowercase()).as_deref() {
        Some("minor") => [0, 2, 3, 5, 7, 8, 10],
        Some("dorian") => [0, 2, 3, 5, 7, 9, 10],
        Some("mixolydian") => [0, 2, 4, 5, 7, 9, 10],
        _ => [0, 2, 4, 5, 7, 9, 11], // major
    }
}

/// Build the 7 pitch classes (0–11) for the current key. Same as sample build_scale_for_mode.
fn build_scale(tonic_pc: i32, mode: Option<&str>) -> [u8; 7] {
    let intervals = mode_intervals(mode);
    let mut scale = [0u8; 7];
    for (i, &step) in intervals.iter().enumerate() {
        scale[i] = ((tonic_pc + step as i32).rem_euclid(12)) as u8;
    }
    scale
}

/// Find the closest scale degree (1–7) and accidental offset (-2..=2) for a note pitch class. Same as sample find_scale_degree.
fn find_scale_degree(note_pc: u8, scale: &[u8; 7]) -> (usize, i8) {
    let note_pc_i = note_pc as i8;
    let mut best_degree = 0usize;
    let mut best_offset: i8 = 127;

    for (d, &target_pc) in scale.iter().enumerate() {
        let target = target_pc as i8;
        let diff_raw = note_pc_i - target;
        let diff_mod = ((diff_raw % 12) + 12) % 12;
        let diff_mod = diff_mod as i8;
        let offset = if diff_mod <= 6 { diff_mod } else { diff_mod - 12 };

        if offset.abs() < best_offset.abs() {
            best_offset = offset;
            best_degree = d;
        }
    }
    (best_degree, best_offset)
}

/// Accidental symbol for scale-degree offset. Use ^ and _ (not # and b) so the glyphs stay small.
fn accidental_symbol(offset: i8) -> &'static str {
    match offset {
        0 => "",
        1 => "^",
        -1 => "_",
        2 => "^^",
        -2 => "__",
        _ => "?",
    }
}

/// Map semitone offset from root (0–11) to (degree 1–7, accidental) for major only.
fn semitone_to_degree_acc(semi: i32) -> (u8, Option<&'static str>) {
    let s = semi.rem_euclid(12);
    match s {
        0 => (1, None),
        1 => (1, Some("^")),
        2 => (2, None),
        3 => (2, Some("^")),
        4 => (3, None),
        5 => (4, None),
        6 => (4, Some("^")), // or b5; ^4 is common
        7 => (5, None),
        8 => (5, Some("^")),
        9 => (6, None),
        10 => (6, Some("^")),
        11 => (7, Some("_")), // leading tone
        _ => (1, None),
    }
}

/// Convert pitch and key to Jianpu digit (1–7), octave dots, and accidental string.
/// Uses mode-aware scale degree when key_mode is provided; otherwise major-only.
pub(super) fn pitch_to_jianpu(
    pitch: &Pitch,
    key_fifths: i32,
    key_mode: Option<&str>,
) -> (u8, i32, Option<&'static str>) {
    let midi = pitch.to_midi();
    let pitch_class = (midi.rem_euclid(12)) as u8;
    let root = key_root_semitone(key_fifths);

    let (degree, accidental_str) = if key_mode.is_some() {
        let scale = build_scale(root, key_mode);
        let (degree_index, offset) = find_scale_degree(pitch_class, &scale);
        let acc = accidental_symbol(offset);
        let degree = (degree_index + 1) as u8;
        let acc_opt = if acc.is_empty() { None } else { Some(acc) };
        (degree, acc_opt)
    } else {
        let semi = (pitch_class as i32 - root).rem_euclid(12);
        let (d, acc) = semitone_to_degree_acc(semi);
        (d, acc)
    };

    let octave_rel = (midi / 12) - 5;
    let octave_dots = octave_rel.clamp(-2, 2);

    (degree, octave_dots, accidental_str)
}

/// Duration in quarter notes to underline count (0–3). Same as sample: eighth=1, sixteenth=2, 32nd=3.
fn duration_to_underline_count(duration_quarters: f64) -> u8 {
    let eighth = 0.5;
    let sixteenth = 0.25;
    let thirty_second = 0.125;
    if (duration_quarters - eighth).abs() < 0.01 {
        1
    } else if duration_quarters <= sixteenth + 0.01 {
        if duration_quarters <= thirty_second + 0.01 {
            3
        } else {
            2
        }
    } else {
        0
    }
}

/// Duration in quarter notes to Jianpu duration style: underlines (0–3), suffix dot, suffix dashes.
pub(super) fn duration_to_jianpu(
    duration_quarters: f64,
    dot: bool,
) -> (u8, bool, u8) {
    let underlines = duration_to_underline_count(duration_quarters);
    let suffix_dashes = if duration_quarters >= 3.99 {
        2
    } else if duration_quarters >= 1.99 {
        1
    } else {
        0
    };
    (underlines, dot, suffix_dashes)
}

/// Build the single Jianpu ASCII font string for one note/rest (e.g. "5'", "3/", "#4//", "0", "1 -").
/// Order: accidental → digit → octave marks → duration slashes → suffix dot → suffix dashes.
fn note_to_jianpu_ascii(
    digit: u8,
    octave_dots: i32,
    accidental: Option<&str>,
    underlines: u8,
    suffix_dot: bool,
    suffix_dashes: u8,
) -> String {
    let mut s = String::new();
    if let Some(acc) = accidental {
        s.push_str(acc);
    }
    s.push(if digit == 0 { '0' } else { (b'0' + digit) as char });
    for _ in 0..octave_dots {
        s.push('\'');
    }
    for _ in 0..(-octave_dots).max(0) {
        s.push(',');
    }
    match underlines {
        1 => s.push('/'),
        2 => s.push_str("//"),
        3 => s.push_str("///"),
        _ => {}
    }
    if suffix_dot {
        s.push('.');
    }
    match suffix_dashes {
        1 => s.push_str(" -"),
        2 => s.push_str(" - - -"),
        _ => {}
    }
    s
}

/// Key label text e.g. "1 = C", "1 = Am". Same as sample key_label_text.
fn key_label_text(key_fifths: i32, key_mode: Option<&str>) -> String {
    fn pitch_class_to_name(pc: i32) -> &'static str {
        match pc.rem_euclid(12) {
            0 => "C", 1 => "C#", 2 => "D", 3 => "Eb", 4 => "E", 5 => "F",
            6 => "F#", 7 => "G", 8 => "Ab", 9 => "A", 10 => "Bb", 11 => "B",
            _ => "?",
        }
    }
    let tonic = key_root_semitone(key_fifths);
    let mode_suffix = match key_mode.map(|s| s.to_lowercase()).as_deref() {
        Some("minor") => "m",
        Some("dorian") => " (Dorian)",
        Some("mixolydian") => " (Mixolydian)",
        _ => "",
    };
    format!("1 = {}{}", pitch_class_to_name(tonic), mode_suffix)
}

/// Draw the jianpu key label (e.g. "1 = C", "1 = Am") at (x, y). Used for measure 0 and mid-score key changes.
pub(super) fn render_key_label(
    svg: &mut SvgBuilder,
    x: f64,
    y: f64,
    key_fifths: i32,
    key_mode: Option<&str>,
) {
    let label = key_label_text(key_fifths, key_mode);
    svg.text(x, y, &label, JIANPU_KEY_LABEL_FONT, "normal", NOTE_COLOR, "start", None);
}

/// Y position for each note on this staff (jianpu center line). Used so lyrics are placed under the note.
pub(super) fn jianpu_note_y_positions(
    measure: &crate::model::Measure,
    _staff_filter: i32,
    staff_y: f64,
) -> Vec<f64> {
    let center_y = staff_y + STAFF_HEIGHT / 2.0;
    vec![center_y; measure.notes.len()]
}

/// Render one measure of Jianpu: key label (first measure), then one `<text>` per note/rest (same pattern as lyrics).
/// Uses the same `note_positions` and same index `i` as lyrics; we center every note head at nx using a single
/// note-head width (excluding duration suffix), so lyrics centered at nx align with the note.
pub(super) fn render_jianpu_measure(
    svg: &mut SvgBuilder,
    measure: &crate::model::Measure,
    staff_y: f64,
    staff_filter: i32,
    key_fifths: i32,
    key_mode: Option<&str>,
    divisions: i32,
    note_positions: &[f64],
    measure_x: f64,
    _measure_w: f64,
    draw_key_label: bool,
) {
    let jianpu_center_y = staff_y + STAFF_HEIGHT / 2.0;
    let div = divisions.max(1) as f64;

    if draw_key_label {
        let label = key_label_text(key_fifths, key_mode);
        let label_x = measure_x;
        let label_y = jianpu_center_y - 20.0;
        svg.text(
            label_x,
            label_y,
            &label,
            JIANPU_KEY_LABEL_FONT,
            "normal",
            NOTE_COLOR,
            "start",
            None,
        );
    }

    // Render one <text> per note/rest (same pattern as lyrics: same index i, same note_positions[i]).
    const X_EPS: f64 = 0.5;
    let mut i = 0;
    while i < measure.notes.len() {
        let note = &measure.notes[i];
        if note.staff.unwrap_or(1) != staff_filter {
            i += 1;
            continue;
        }
        if i >= note_positions.len() {
            break;
        }
        let nx = note_positions[i];

        let group_indices: Vec<usize> = (0..measure.notes.len())
            .filter(|&k| measure.notes[k].staff.unwrap_or(1) == staff_filter)
            .filter(|&k| {
                k < note_positions.len() && (note_positions[k] - nx).abs() <= X_EPS
            })
            .collect();

        let group_len = group_indices.len();
        let group_end = group_indices.iter().max().copied().unwrap_or(i) + 1;

        if group_indices.iter().min().copied() != Some(i) {
            i = group_end;
            continue;
        }

        let w = jianpu_note_head_width(JIANPU_FONT_SIZE);
        if note.rest && group_len == 1 {
            let duration_quarters = note.duration as f64 / div;
            let (underlines, dot, dashes) = duration_to_jianpu(duration_quarters, note.dot);
            let ascii_str = note_to_jianpu_ascii(0, 0, None, underlines, dot, dashes);
            svg.jianpu_note_text_centered(nx, jianpu_center_y, &ascii_str, w, 0.0, "Jianpu", JIANPU_FONT_SIZE, NOTE_COLOR);
            i = group_end;
            continue;
        }

        if note.grace {
            for (idx, &k) in group_indices.iter().enumerate() {
                let grace_note = &measure.notes[k];
                if let Some(ref pitch) = grace_note.pitch {
                    let (digit, octave_dots, accidental) = pitch_to_jianpu(pitch, key_fifths, key_mode);
                    let y = if group_len <= 1 {
                        jianpu_center_y
                    } else {
                        let offset = idx as f64 - (group_len as f64 - 1.0) / 2.0;
                        jianpu_center_y + offset * jianpu_chord_stack_spacing()
                    };
                    let ascii_str = note_to_jianpu_ascii(digit, octave_dots, accidental, 1, false, 0);
                    svg.jianpu_note_text_centered(nx, y, &ascii_str, w, 0.0, "Jianpu", JIANPU_FONT_SIZE, NOTE_COLOR);
                }
            }
            i = group_end;
            continue;
        }

        let mut chord_notes: Vec<(usize, &crate::model::Note)> = group_indices
            .iter()
            .map(|&k| (k, &measure.notes[k]))
            .collect();
        chord_notes.sort_by_key(|(_, n)| {
            let rest_last = if n.rest { 1 } else { 0 };
            let midi = n.pitch.as_ref().map(|p| p.to_midi()).unwrap_or(0);
            (rest_last, midi)
        });

        for (chord_idx, &(_k, n)) in chord_notes.iter().enumerate() {
            let y = if chord_notes.len() <= 1 {
                jianpu_center_y
            } else {
                let offset = chord_idx as f64 - (chord_notes.len() as f64 - 1.0) / 2.0;
                jianpu_center_y + offset * jianpu_chord_stack_spacing()
            };

            if n.rest {
                let duration_quarters = n.duration as f64 / div;
                let (underlines, dot, dashes) = duration_to_jianpu(duration_quarters, n.dot);
                let ascii_str = note_to_jianpu_ascii(0, 0, None, underlines, dot, dashes);
                svg.jianpu_note_text_centered(nx, y, &ascii_str, w, 0.0, "Jianpu", JIANPU_FONT_SIZE, NOTE_COLOR);
            } else if let Some(ref pitch) = n.pitch {
                let (digit, octave_dots, accidental) = pitch_to_jianpu(pitch, key_fifths, key_mode);
                let duration_quarters = n.duration as f64 / div;
                let (underlines, suffix_dot, suffix_dashes) = duration_to_jianpu(duration_quarters, n.dot);
                let ascii_str = note_to_jianpu_ascii(
                    digit, octave_dots, accidental,
                    underlines, suffix_dot, suffix_dashes,
                );
                svg.jianpu_note_text_centered(nx, y, &ascii_str, w, 0.0, "Jianpu", JIANPU_FONT_SIZE, NOTE_COLOR);
            }
        }
        i = group_end;
    }
}
