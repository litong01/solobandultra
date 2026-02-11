//! Slur rendering (OSMD-style double-bezier filled shape).

use crate::model::*;
use super::svg_builder::SvgBuilder;
use super::beat_map::{pitch_to_staff_y, note_x_positions_from_beat_map};

const SLUR_COLOR: &str = "#1a1a1a";
const SLUR_NOTEHEAD_Y_OFFSET: f64 = 3.0;
const SLUR_ENDPOINT_THICKNESS: f64 = 0.5;
const SLUR_MID_THICKNESS: f64 = 1.5;
const SLUR_HEIGHT_FACTOR: f64 = 0.15;
const SLUR_MIN_HEIGHT: f64 = 5.0;
const SLUR_MAX_HEIGHT: f64 = 25.0;

/// Recorded position of a slur start event.
#[derive(Clone, Debug)]
pub(super) struct SlurStart {
    pub(super) x: f64,
    pub(super) y: f64,
    pub(super) stem_up: bool,
    pub(super) placement: Option<String>,
    pub(super) staff_y: f64,
}

/// Collect slur positions for all notes in a single measure/staff and
/// process start/stop events.
pub(super) fn collect_and_render_slurs_for_measure(
    svg: &mut SvgBuilder,
    measure: &Measure,
    staff_y: f64,
    clef: Option<&Clef>,
    divisions: i32,
    transpose_octave: i32,
    staff_filter: Option<i32>,
    beat_x_map: &[(f64, f64)],
    open_slurs: &mut std::collections::HashMap<i32, SlurStart>,
) {
    if measure.notes.is_empty() {
        return;
    }

    let note_positions = note_x_positions_from_beat_map(&measure.notes, divisions, beat_x_map);

    for (i, note) in measure.notes.iter().enumerate() {
        if let Some(sf) = staff_filter {
            if note.staff.unwrap_or(1) != sf { continue; }
        }

        if note.slurs.is_empty() || note.rest { continue; }

        let nx = note_positions[i];

        let (note_y, stem_up) = if let Some(ref pitch) = note.pitch {
            let ny = staff_y + pitch_to_staff_y(pitch, clef, transpose_octave);
            let su = match note.stem.as_deref() {
                Some("up") => true,
                Some("down") => false,
                _ => ny >= staff_y + 20.0,
            };
            (ny, su)
        } else {
            (staff_y + 20.0, true)
        };

        let mut sorted_events: Vec<&crate::model::SlurEvent> = note.slurs.iter().collect();
        sorted_events.sort_by(|a, b| {
            let order = |t: &str| if t == "stop" { 0 } else { 1 };
            order(&a.slur_type).cmp(&order(&b.slur_type))
        });

        for ev in &sorted_events {
            match ev.slur_type.as_str() {
                "start" => {
                    open_slurs.insert(ev.number, SlurStart {
                        x: nx,
                        y: note_y,
                        stem_up,
                        placement: ev.placement.clone(),
                        staff_y,
                    });
                }
                "stop" => {
                    if let Some(start) = open_slurs.remove(&ev.number) {
                        render_slur(svg, &start, nx, note_y, stem_up);
                    }
                }
                _ => {}
            }
        }
    }
}

/// Draw a slur between two note positions.
fn render_slur(
    svg: &mut SvgBuilder,
    start: &SlurStart,
    end_x: f64,
    end_y: f64,
    _end_stem_up: bool,
) {
    let above = match start.placement.as_deref() {
        Some("above") => true,
        Some("below") => false,
        _ => !start.stem_up,
    };

    let y_dir = if above { -1.0 } else { 1.0 };

    let sx = start.x;
    let sy = start.y + y_dir * SLUR_NOTEHEAD_Y_OFFSET;
    let ex = end_x;
    let ey = end_y + y_dir * SLUR_NOTEHEAD_Y_OFFSET;

    let dx = (ex - sx).abs().max(1.0);
    let height = (dx * SLUR_HEIGHT_FACTOR).clamp(SLUR_MIN_HEIGHT, SLUR_MAX_HEIGHT);
    let mid_y = (sy + ey) / 2.0;

    let cp1x = sx + dx * 0.25;
    let cp1y = mid_y + y_dir * height;
    let cp2x = sx + dx * 0.75;
    let cp2y = mid_y + y_dir * height;

    let ep_off = SLUR_ENDPOINT_THICKNESS * y_dir;
    let cp_off = SLUR_MID_THICKNESS * y_dir;

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

    svg.path(&path, SLUR_COLOR, "none", 0.0);
}

/// Draw continuation slurs for any still-open slurs at the end of a system.
pub(super) fn render_open_slur_continuations(
    svg: &mut SvgBuilder,
    open_slurs: &std::collections::HashMap<i32, SlurStart>,
    system_x_end: f64,
) {
    for (_number, start) in open_slurs.iter() {
        let end_y = start.y;
        render_slur(svg, start, system_x_end, end_y, start.stem_up);
    }
}
