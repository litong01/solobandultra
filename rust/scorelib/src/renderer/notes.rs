//! Note, rest, beam, accidental, and ledger line rendering.

use crate::model::*;
use super::constants::*;
use super::glyphs::*;
use super::svg_builder::{SvgBuilder, vexflow_outline_to_svg, vf_outline_to_svg};
use super::beat_map::{pitch_to_staff_y, is_filled_note, note_x_positions_from_beat_map};

// ── Grace note constants ─────────────────────────────────────────────
const GRACE_SCALE: f64 = 0.66;
const GRACE_NOTEHEAD_RX: f64 = NOTEHEAD_RX * GRACE_SCALE;
const GRACE_NOTEHEAD_RY: f64 = NOTEHEAD_RY * GRACE_SCALE;
const GRACE_STEM_LENGTH: f64 = STEM_LENGTH * GRACE_SCALE;
const GRACE_STEM_WIDTH: f64 = STEM_WIDTH * 0.85;
const GRACE_FLAG_GLYPH_SCALE: f64 = FLAG_GLYPH_SCALE * GRACE_SCALE;

pub(super) fn render_notes(
    svg: &mut SvgBuilder,
    measure: &Measure,
    staff_y: f64,
    clef: Option<&Clef>,
    divisions: i32,
    transpose_octave: i32,
    staff_filter: Option<i32>,
    beat_x_map: &[(f64, f64)],
    measure_x: f64,
    measure_w: f64,
) {
    if measure.notes.is_empty() {
        return;
    }

    let note_positions = note_x_positions_from_beat_map(&measure.notes, divisions, beat_x_map);
    let measure_center_x = measure_x + measure_w / 2.0;

    let staff_note_count = measure.notes.iter().filter(|n| {
        if n.grace { return false; }
        if let Some(sf) = staff_filter {
            n.staff.unwrap_or(1) == sf
        } else {
            true
        }
    }).count();
    let is_solo_rest = staff_note_count == 1
        && measure.notes.iter().any(|n| {
            if n.grace { return false; }
            let on_staff = if let Some(sf) = staff_filter {
                n.staff.unwrap_or(1) == sf
            } else { true };
            on_staff && n.rest
        });

    let beam_groups = find_beam_groups(measure, staff_filter);

    for (i, note) in measure.notes.iter().enumerate() {
        if let Some(sf) = staff_filter {
            let note_staff = note.staff.unwrap_or(1);
            if note_staff != sf { continue; }
        }

        let nx = note_positions[i];

        if note.rest {
            let rest_x = if note.measure_rest || note.note_type.is_none() || is_solo_rest {
                measure_center_x
            } else {
                nx
            };
            render_rest(svg, rest_x, staff_y, note.note_type.as_deref(), note.measure_rest);
            continue;
        }

        if note.grace {
            if let Some(ref pitch) = note.pitch {
                let note_y = staff_y + pitch_to_staff_y(pitch, clef, transpose_octave);
                render_grace_note(svg, note, nx, note_y, staff_y);
            }
            continue;
        }

        if let Some(ref pitch) = note.pitch {
            let note_y = staff_y + pitch_to_staff_y(pitch, clef, transpose_octave);

            render_ledger_lines(svg, nx, note_y, staff_y);

            let filled = is_filled_note(note.note_type.as_deref());
            let is_whole = note.note_type.as_deref() == Some("whole");
            svg.notehead(nx, note_y, filled, is_whole);

            if note.dot {
                svg.circle(nx + NOTEHEAD_RX + 4.0, note_y - 1.5, 1.8, NOTE_COLOR);
            }

            if let Some(ref acc) = note.accidental {
                render_accidental(svg, nx - NOTEHEAD_RX - 4.0, note_y, acc);
            }

            if !is_whole {
                let in_beam = note.beams.iter().any(|b|
                    b.beam_type == "begin" || b.beam_type == "continue" || b.beam_type == "end");

                if !in_beam {
                    let stem_up = match note.stem.as_deref() {
                        Some("up") => true,
                        Some("down") => false,
                        _ => note_y >= staff_y + 20.0,
                    };

                    let flag_count = note.note_type.as_deref().map_or(0, |nt| match nt {
                        "eighth" => 1,
                        "16th" => 2,
                        "32nd" => 3,
                        "64th" => 4,
                        _ => 0,
                    });

                    let stem_extra = match flag_count {
                        2 => 4.0,
                        3 => 9.0,
                        4 => 13.0,
                        _ => 0.0,
                    };
                    let stem_len = STEM_LENGTH + stem_extra;

                    let (sx, sy1, sy2) = if stem_up {
                        (nx + NOTEHEAD_RX - 1.0, note_y, note_y - stem_len)
                    } else {
                        (nx - NOTEHEAD_RX + 1.0, note_y, note_y + stem_len)
                    };
                    svg.line(sx, sy1, sx, sy2, NOTE_COLOR, STEM_WIDTH);

                    if flag_count > 0 {
                        render_flags(svg, sx, sy2, flag_count, stem_up);
                    }
                }
            }
        }
    }

    for group in &beam_groups {
        render_beam_group(svg, measure, &note_positions, staff_y, clef, transpose_octave, group);
    }
}

// ── Grace note rendering ────────────────────────────────────────────

fn render_grace_note(
    svg: &mut SvgBuilder,
    note: &Note,
    nx: f64,
    note_y: f64,
    staff_y: f64,
) {
    let rx = GRACE_NOTEHEAD_RX;
    let ry = GRACE_NOTEHEAD_RY;

    svg.elements.push(format!(
        r#"<ellipse cx="{:.1}" cy="{:.1}" rx="{:.1}" ry="{:.1}" fill="{}" stroke="none" stroke-width="0" transform="rotate(-15,{:.1},{:.1})"/>"#,
        nx, note_y, rx, ry, NOTE_COLOR, nx, note_y
    ));

    if let Some(ref acc) = note.accidental {
        render_accidental(svg, nx - rx - 3.0, note_y, acc);
    }

    let stem_up = match note.stem.as_deref() {
        Some("up") => true,
        Some("down") => false,
        _ => note_y >= staff_y + 20.0,
    };

    let flag_count = note.note_type.as_deref().map_or(1, |nt| match nt {
        "eighth" => 1,
        "16th" => 2,
        "32nd" => 3,
        "64th" => 4,
        _ => 1,
    });

    let stem_extra = match flag_count {
        2 => 3.0,
        3 => 6.0,
        4 => 9.0,
        _ => 0.0,
    };
    let stem_len = GRACE_STEM_LENGTH + stem_extra;

    let (sx, sy1, sy2) = if stem_up {
        (nx + rx - 0.5, note_y, note_y - stem_len)
    } else {
        (nx - rx + 0.5, note_y, note_y + stem_len)
    };
    svg.line(sx, sy1, sx, sy2, NOTE_COLOR, GRACE_STEM_WIDTH);

    if flag_count > 0 {
        render_grace_flags(svg, sx, sy2, flag_count, stem_up);
    }

    if note.grace_slash {
        let slash_len = 7.0;
        let slash_mid_y = if stem_up {
            note_y - stem_len * 0.4
        } else {
            note_y + stem_len * 0.4
        };
        let (x1, y1) = (sx - slash_len * 0.5, slash_mid_y + slash_len * 0.4);
        let (x2, y2) = (sx + slash_len * 0.5, slash_mid_y - slash_len * 0.4);
        svg.line(x1, y1, x2, y2, NOTE_COLOR, 1.0);
    }
}

fn render_grace_flags(svg: &mut SvgBuilder, stem_x: f64, stem_end_y: f64, count: usize, stem_up: bool) {
    let outline = match (count, stem_up) {
        (1, true)  => FLAG_8TH_UP,
        (1, false) => FLAG_8TH_DOWN,
        (2, true)  => FLAG_16TH_UP,
        (2, false) => FLAG_16TH_DOWN,
        (3, true)  => FLAG_32ND_UP,
        (3, false) => FLAG_32ND_DOWN,
        (4, true)  => FLAG_64TH_UP,
        (4, false) => FLAG_64TH_DOWN,
        _ => return,
    };

    let s = GRACE_FLAG_GLYPH_SCALE;
    let path = vexflow_outline_to_svg(outline, s, stem_x, stem_end_y);
    svg.path(&path, NOTE_COLOR, NOTE_COLOR, 0.2);
}

// ── Rest rendering ──────────────────────────────────────────────────

fn render_rest(svg: &mut SvgBuilder, x: f64, staff_y: f64, note_type: Option<&str>, measure_rest: bool) {
    if measure_rest || note_type.is_none() {
        svg.rect(x - 7.0, staff_y + 10.0, 14.0, 5.0, REST_COLOR, "none", 0.0);
        return;
    }

    match note_type.unwrap() {
        "whole" => {
            svg.rect(x - 7.0, staff_y + 10.0, 14.0, 5.0, REST_COLOR, "none", 0.0);
        }
        "half" => {
            svg.rect(x - 7.0, staff_y + 15.0, 14.0, 5.0, REST_COLOR, "none", 0.0);
        }
        "quarter" => {
            let path = vf_outline_to_svg(VF_QUARTER_REST, VF_GLYPH_SCALE);
            let gx = x - 4.0;
            let gy = staff_y + 20.0;
            svg.elements.push(format!(
                r#"<path d="{}" fill="{}" transform="translate({:.1},{:.1})"/>"#,
                path, REST_COLOR, gx, gy
            ));
        }
        "eighth" => {
            let path = vf_outline_to_svg(VF_EIGHTH_REST, VF_GLYPH_SCALE);
            let gx = x - 5.0;
            let gy = staff_y + 20.0;
            svg.elements.push(format!(
                r#"<path d="{}" fill="{}" transform="translate({:.1},{:.1})"/>"#,
                path, REST_COLOR, gx, gy
            ));
        }
        "16th" => {
            let path = vf_outline_to_svg(VF_16TH_REST, VF_GLYPH_SCALE);
            let gx = x - 6.0;
            let gy = staff_y + 20.0;
            svg.elements.push(format!(
                r#"<path d="{}" fill="{}" transform="translate({:.1},{:.1})"/>"#,
                path, REST_COLOR, gx, gy
            ));
        }
        _ => {
            svg.rect(x - 7.0, staff_y + 10.0, 14.0, 5.0, REST_COLOR, "none", 0.0);
        }
    }
}

// ── Accidental rendering ────────────────────────────────────────────

fn render_accidental(svg: &mut SvgBuilder, x: f64, y: f64, accidental: &str) {
    match accidental {
        "sharp" => svg.sharp_glyph(x - 4.5, y),
        "flat" => svg.flat_glyph(x - 3.5, y),
        "natural" => svg.natural_glyph(x - 3.5, y),
        "double-sharp" => svg.double_sharp_glyph(x - 5.0, y),
        "flat-flat" => svg.double_flat_glyph(x - 6.5, y),
        _ => {}
    }
}

// ── Flag rendering ──────────────────────────────────────────────────

fn render_flags(svg: &mut SvgBuilder, stem_x: f64, stem_end_y: f64, count: usize, stem_up: bool) {
    let outline = match (count, stem_up) {
        (1, true)  => FLAG_8TH_UP,
        (1, false) => FLAG_8TH_DOWN,
        (2, true)  => FLAG_16TH_UP,
        (2, false) => FLAG_16TH_DOWN,
        (3, true)  => FLAG_32ND_UP,
        (3, false) => FLAG_32ND_DOWN,
        (4, true)  => FLAG_64TH_UP,
        (4, false) => FLAG_64TH_DOWN,
        _ => return,
    };

    let s = FLAG_GLYPH_SCALE;
    let path = vexflow_outline_to_svg(outline, s, stem_x, stem_end_y);
    svg.path(&path, NOTE_COLOR, NOTE_COLOR, 0.3);
}

// ── Ledger lines ────────────────────────────────────────────────────

fn render_ledger_lines(svg: &mut SvgBuilder, x: f64, note_y: f64, staff_y: f64) {
    let top = staff_y;
    let bottom = staff_y + STAFF_HEIGHT;

    if note_y < top {
        let mut y = top - STAFF_LINE_SPACING;
        while y >= note_y - 1.0 {
            svg.line(
                x - NOTEHEAD_RX - LEDGER_LINE_EXTEND,
                y,
                x + NOTEHEAD_RX + LEDGER_LINE_EXTEND,
                y,
                STAFF_COLOR, LEDGER_LINE_WIDTH,
            );
            y -= STAFF_LINE_SPACING;
        }
    }

    if note_y > bottom {
        let mut y = bottom + STAFF_LINE_SPACING;
        while y <= note_y + 1.0 {
            svg.line(
                x - NOTEHEAD_RX - LEDGER_LINE_EXTEND,
                y,
                x + NOTEHEAD_RX + LEDGER_LINE_EXTEND,
                y,
                STAFF_COLOR, LEDGER_LINE_WIDTH,
            );
            y += STAFF_LINE_SPACING;
        }
    }
}

// ── Beam rendering ──────────────────────────────────────────────────

fn find_beam_groups(measure: &Measure, staff_filter: Option<i32>) -> Vec<Vec<usize>> {
    let mut groups: Vec<Vec<usize>> = Vec::new();
    let mut current_group: Vec<usize> = Vec::new();

    for (i, note) in measure.notes.iter().enumerate() {
        if note.chord || note.rest || note.grace {
            continue;
        }
        if let Some(sf) = staff_filter {
            if note.staff.unwrap_or(1) != sf { continue; }
        }
        let has_beam_begin = note.beams.iter().any(|b| b.number == 1 && b.beam_type == "begin");
        let has_beam_cont = note.beams.iter().any(|b| b.number == 1 && b.beam_type == "continue");
        let has_beam_end = note.beams.iter().any(|b| b.number == 1 && b.beam_type == "end");

        if has_beam_begin {
            current_group = vec![i];
        } else if has_beam_cont {
            current_group.push(i);
        } else if has_beam_end {
            current_group.push(i);
            if current_group.len() >= 2 {
                groups.push(current_group.clone());
            }
            current_group.clear();
        }
    }

    groups
}

fn render_beam_group(
    svg: &mut SvgBuilder,
    measure: &Measure,
    note_positions: &[f64],
    staff_y: f64,
    clef: Option<&Clef>,
    transpose_octave: i32,
    group: &[usize],
) {
    if group.len() < 2 {
        return;
    }

    struct BeamNote { x: f64, note_y: f64, stem_x: f64 }
    let mut notes: Vec<BeamNote> = Vec::new();

    for &idx in group {
        let note = &measure.notes[idx];
        let nx = note_positions[idx];
        if let Some(ref pitch) = note.pitch {
            let note_y = staff_y + pitch_to_staff_y(pitch, clef, transpose_octave);
            notes.push(BeamNote { x: nx, note_y, stem_x: 0.0 });
        }
    }
    if notes.len() < 2 { return; }

    let avg_y: f64 = notes.iter().map(|n| n.note_y).sum::<f64>() / notes.len() as f64;
    let middle_line = staff_y + 20.0;

    let first_note = &measure.notes[group[0]];
    let stem_up = match first_note.stem.as_deref() {
        Some("up") => true,
        Some("down") => false,
        _ => avg_y >= middle_line,
    };

    for n in &mut notes {
        n.stem_x = if stem_up { n.x + NOTEHEAD_RX - 1.0 } else { n.x - NOTEHEAD_RX + 1.0 };
    }

    let first_stem_end = if stem_up { notes.first().unwrap().note_y - STEM_LENGTH }
                         else { notes.first().unwrap().note_y + STEM_LENGTH };
    let last_stem_end  = if stem_up { notes.last().unwrap().note_y - STEM_LENGTH }
                         else { notes.last().unwrap().note_y + STEM_LENGTH };

    let first_x = notes.first().unwrap().stem_x;
    let last_x  = notes.last().unwrap().stem_x;
    let beam_dx = last_x - first_x;

    let slope = if beam_dx.abs() > 0.1 {
        ((last_stem_end - first_stem_end) / beam_dx).clamp(-0.5, 0.5)
    } else { 0.0 };
    let beam_y = |sx: f64| first_stem_end + slope * (sx - first_x);

    let min_stem = 18.0;
    let mut beam_shift = 0.0_f64;
    for n in &notes {
        let by = beam_y(n.stem_x) + beam_shift;
        let stem_len = (n.note_y - by).abs();
        if stem_len < min_stem {
            let needed = min_stem - stem_len;
            if stem_up { beam_shift -= needed; } else { beam_shift += needed; }
        }
    }

    let beam_y_adj = |sx: f64| beam_y(sx) + beam_shift;

    for n in &notes {
        let by = beam_y_adj(n.stem_x);
        svg.line(n.stem_x, n.note_y, n.stem_x, by, NOTE_COLOR, STEM_WIDTH);
    }

    let by_first = beam_y_adj(first_x);
    let by_last  = beam_y_adj(last_x);
    svg.beam_line(first_x, by_first, last_x, by_last, BEAM_THICKNESS);

    let has_16th = group.iter().any(|&idx| {
        measure.notes[idx].beams.iter().any(|b| b.number == 2)
    });
    if has_16th && notes.len() >= 2 {
        let offset = if stem_up { BEAM_THICKNESS + 3.0 } else { -(BEAM_THICKNESS + 3.0) };
        svg.beam_line(
            first_x, by_first + offset,
            last_x, by_last + offset,
            BEAM_THICKNESS,
        );
    }
}
