//! Staff, clef, key/time signature, header, direction, harmony, and barline rendering.

use crate::model::*;
use super::constants::*;
use super::glyphs::*;
use super::svg_builder::{SvgBuilder, vexflow_outline_to_svg};

// ═══════════════════════════════════════════════════════════════════════
// Header rendering
// ═══════════════════════════════════════════════════════════════════════

pub(super) fn render_header(svg: &mut SvgBuilder, score: &Score, page_width: f64) {
    let center_x = page_width / 2.0;

    if let Some(ref title) = score.title {
        let style = score.title_style.as_ref();
        let size = style.and_then(|s| s.font_size).unwrap_or(22.0);
        let weight = style.and_then(|s| s.font_weight.as_deref()).unwrap_or("bold");
        let family = style.and_then(|s| s.font_family.as_deref());
        let font_style = style.and_then(|s| s.font_style.as_deref());
        svg.styled_text(center_x, PAGE_MARGIN_TOP + 22.0, title, size, weight,
                        HEADER_COLOR, "middle", family, font_style);
    }

    if let Some(ref subtitle) = score.subtitle {
        let style = score.subtitle_style.as_ref();
        let size = style.and_then(|s| s.font_size).unwrap_or(14.0);
        let weight = style.and_then(|s| s.font_weight.as_deref()).unwrap_or("normal");
        let family = style.and_then(|s| s.font_family.as_deref());
        let font_style = style.and_then(|s| s.font_style.as_deref());
        svg.styled_text(center_x, PAGE_MARGIN_TOP + 40.0, subtitle, size, weight,
                        HEADER_COLOR, "middle", family, font_style);
    }

    if let Some(ref composer) = score.composer {
        let label = if let Some(ref arranger) = score.arranger {
            format!("{}\nArr. {}", composer, arranger)
        } else {
            composer.clone()
        };
        let style = score.composer_style.as_ref();
        let size = style.and_then(|s| s.font_size).unwrap_or(11.0);
        let weight = style.and_then(|s| s.font_weight.as_deref()).unwrap_or("normal");
        let family = style.and_then(|s| s.font_family.as_deref());
        let font_style = style.and_then(|s| s.font_style.as_deref());
        svg.styled_text(page_width - PAGE_MARGIN_RIGHT, PAGE_MARGIN_TOP + 55.0,
                        &label, size, weight, HEADER_COLOR, "end", family, font_style);
    }
}

// ═══════════════════════════════════════════════════════════════════════
// Staff rendering
// ═══════════════════════════════════════════════════════════════════════

pub(super) fn render_brace(svg: &mut SvgBuilder, x: f64, top_y: f64, bottom_y: f64) {
    let mid_y = (top_y + bottom_y) / 2.0;
    let h = bottom_y - top_y;
    let w = BRACE_WIDTH;

    let path = format!(
        "M{:.1},{:.1} C{:.1},{:.1} {:.1},{:.1} {:.1},{:.1} \
         C{:.1},{:.1} {:.1},{:.1} {:.1},{:.1}",
        x, top_y,
        x, top_y + h * 0.28,
        x - w, mid_y - h * 0.08,
        x - w, mid_y,
        x - w, mid_y + h * 0.08,
        x, bottom_y - h * 0.28,
        x, bottom_y,
    );
    svg.path(&path, "none", NOTE_COLOR, 2.5);
}

pub(super) fn render_staff_lines(svg: &mut SvgBuilder, x1: f64, x2: f64, staff_y: f64) {
    for i in 0..5 {
        let y = staff_y + i as f64 * STAFF_LINE_SPACING;
        svg.line(x1, y, x2, y, STAFF_COLOR, STAFF_LINE_WIDTH);
    }
}

// ═══════════════════════════════════════════════════════════════════════
// Clef rendering
// ═══════════════════════════════════════════════════════════════════════

pub(super) fn render_clef(svg: &mut SvgBuilder, x: f64, staff_y: f64, clef: &Clef) {
    match clef.sign.as_str() {
        "G" => {
            let cy = staff_y + 30.0;
            svg.treble_clef(x + 10.0, cy);
            if clef.octave_change == Some(-1) {
                svg.text(x + 10.0, staff_y + STAFF_HEIGHT + 16.0,
                         "8", 9.0, "normal", STAFF_COLOR, "middle");
            }
        }
        "F" => {
            let cy = staff_y + 10.0;
            svg.bass_clef(x + 10.0, cy);
        }
        "C" => {
            let line_y = staff_y + (5 - clef.line) as f64 * STAFF_LINE_SPACING;
            svg.alto_clef(x + 10.0, line_y);
        }
        _ => {}
    }
}

// ═══════════════════════════════════════════════════════════════════════
// Key signature rendering
// ═══════════════════════════════════════════════════════════════════════

pub(super) fn render_key_signature(
    svg: &mut SvgBuilder, x: f64, staff_y: f64,
    key: &Key, clef: Option<&Clef>,
) {
    if key.fifths == 0 {
        return;
    }

    let is_treble = clef.map_or(true, |c| c.sign == "G");

    if key.fifths > 0 {
        let positions_treble: &[f64] = &[0.0, 15.0, -5.0, 10.0, 25.0, 5.0, 20.0];
        let positions_bass: &[f64]   = &[10.0, 25.0, 5.0, 20.0, 35.0, 15.0, 30.0];
        let positions = if is_treble { positions_treble } else { positions_bass };
        for i in 0..key.fifths.min(7) as usize {
            let sx = x + i as f64 * KEY_SIG_SHARP_SPACE;
            let sy = staff_y + positions[i];
            svg.sharp_glyph(sx, sy);
        }
    } else {
        let positions_treble: &[f64] = &[20.0, 5.0, 25.0, 10.0, 30.0, 15.0, 35.0];
        let positions_bass: &[f64]   = &[30.0, 15.0, 35.0, 20.0, 40.0, 25.0, 45.0];
        let positions = if is_treble { positions_treble } else { positions_bass };
        for i in 0..key.fifths.unsigned_abs().min(7) as usize {
            let sx = x + i as f64 * KEY_SIG_FLAT_SPACE;
            let sy = staff_y + positions[i];
            svg.flat_glyph(sx, sy);
        }
    }
}

/// Return staff-line positions (in half-space units) for sharp key signatures.
pub(super) fn sharp_positions(clef: Option<&Clef>) -> Vec<i32> {
    let is_treble = clef.map_or(true, |c| c.sign == "G");
    if is_treble {
        vec![0, 3, -1, 2, 5, 1, 4]
    } else {
        vec![2, 5, 1, 4, 7, 3, 6]
    }
}

/// Return staff-line positions (in half-space units) for flat key signatures.
pub(super) fn flat_positions(clef: Option<&Clef>) -> Vec<i32> {
    let is_treble = clef.map_or(true, |c| c.sign == "G");
    if is_treble {
        vec![4, 1, 5, 2, 6, 3, 7]
    } else {
        vec![6, 3, 7, 4, 8, 5, 9]
    }
}

/// Render a natural sign at the given position.
pub(super) fn render_natural_sign(svg: &mut SvgBuilder, x: f64, y: f64) {
    svg.natural_glyph(x, y);
}

// ═══════════════════════════════════════════════════════════════════════
// Tempo / direction rendering
// ═══════════════════════════════════════════════════════════════════════

pub(super) fn render_tempo_marking(svg: &mut SvgBuilder, x: f64, staff_y: f64, dir: &Direction) {
    let ty = staff_y - 16.0;

    let (bpm, beat_unit, dotted) = if let Some(ref metro) = dir.metronome {
        (metro.per_minute as f64, metro.beat_unit.as_str(), metro.dotted)
    } else if let Some(tempo) = dir.sound_tempo {
        (tempo, "quarter", false)
    } else {
        return;
    };

    #[allow(unused_assignments)]
    let mut note_end_x = x;
    match beat_unit {
        "quarter" => {
            svg.elements.push(format!(
                "<ellipse cx=\"{:.1}\" cy=\"{:.1}\" rx=\"3.8\" ry=\"2.8\" fill=\"{}\" stroke=\"none\" transform=\"rotate(-15,{:.1},{:.1})\"/>",
                x + 3.8, ty, NOTE_COLOR, x + 3.8, ty
            ));
            svg.line(x + 7.0, ty, x + 7.0, ty - 16.0, NOTE_COLOR, 1.0);
            note_end_x = x + 10.0;
        }
        "half" => {
            svg.elements.push(format!(
                "<ellipse cx=\"{:.1}\" cy=\"{:.1}\" rx=\"3.8\" ry=\"2.8\" fill=\"none\" stroke=\"{}\" stroke-width=\"1.2\" transform=\"rotate(-15,{:.1},{:.1})\"/>",
                x + 3.8, ty, NOTE_COLOR, x + 3.8, ty
            ));
            svg.line(x + 7.0, ty, x + 7.0, ty - 16.0, NOTE_COLOR, 1.0);
            note_end_x = x + 10.0;
        }
        "eighth" => {
            svg.elements.push(format!(
                "<ellipse cx=\"{:.1}\" cy=\"{:.1}\" rx=\"3.8\" ry=\"2.8\" fill=\"{}\" stroke=\"none\" transform=\"rotate(-15,{:.1},{:.1})\"/>",
                x + 3.8, ty, NOTE_COLOR, x + 3.8, ty
            ));
            svg.line(x + 7.0, ty, x + 7.0, ty - 16.0, NOTE_COLOR, 1.0);
            svg.elements.push(format!(
                "<path d=\"M{:.1},{:.1} c 0.7,1.4 2.8,3.5 5,7 c 1.4,2.1 1.4,4.2 -0.7,5.6\" fill=\"none\" stroke=\"{}\" stroke-width=\"1.0\"/>",
                x + 7.0, ty - 16.0, NOTE_COLOR
            ));
            note_end_x = x + 14.0;
        }
        "whole" => {
            svg.elements.push(format!(
                "<ellipse cx=\"{:.1}\" cy=\"{:.1}\" rx=\"4.5\" ry=\"3.0\" fill=\"none\" stroke=\"{}\" stroke-width=\"1.5\" transform=\"rotate(-15,{:.1},{:.1})\"/>",
                x + 4.5, ty, NOTE_COLOR, x + 4.5, ty
            ));
            note_end_x = x + 11.0;
        }
        _ => {
            svg.elements.push(format!(
                "<ellipse cx=\"{:.1}\" cy=\"{:.1}\" rx=\"3.8\" ry=\"2.8\" fill=\"{}\" stroke=\"none\" transform=\"rotate(-15,{:.1},{:.1})\"/>",
                x + 3.8, ty, NOTE_COLOR, x + 3.8, ty
            ));
            svg.line(x + 7.0, ty, x + 7.0, ty - 16.0, NOTE_COLOR, 1.0);
            note_end_x = x + 10.0;
        }
    }

    if dotted {
        svg.elements.push(format!(
            "<circle cx=\"{:.1}\" cy=\"{:.1}\" r=\"1.2\" fill=\"{}\"/>",
            note_end_x, ty - 1.0, NOTE_COLOR
        ));
        note_end_x += 3.0;
    }

    let text = format!(" = {}", bpm as i32);
    svg.text(note_end_x, ty + 4.0, &text, 12.0, "bold", NOTE_COLOR, "start");
}

pub(super) fn render_segno(svg: &mut SvgBuilder, x: f64, staff_y: f64) {
    let y = staff_y - 14.0;
    let path = vexflow_outline_to_svg(SEGNO_GLYPH, SEGNO_GLYPH_SCALE, x, y);
    svg.path(&path, NOTE_COLOR, NOTE_COLOR, 0.3);
}

pub(super) fn render_coda(svg: &mut SvgBuilder, x: f64, staff_y: f64) {
    let y = staff_y - 14.0;
    let path = vexflow_outline_to_svg(CODA_GLYPH, CODA_GLYPH_SCALE, x, y);
    svg.path(&path, NOTE_COLOR, NOTE_COLOR, 0.3);
}

/// Check whether a text string is a jump/navigation instruction.
pub(super) fn is_jump_text(text: &str) -> bool {
    let lower = text.trim().to_lowercase();
    lower.starts_with("d.c.")
        || lower.starts_with("d.s.")
        || lower.starts_with("d. c.")
        || lower.starts_with("d. s.")
        || lower == "fine"
        || lower.starts_with("to coda")
        || lower.starts_with("tocoda")
        || lower == "coda"
        || lower == "segno"
        || lower.starts_with("da capo")
        || lower.starts_with("dal segno")
}

pub(super) fn render_jump_text(svg: &mut SvgBuilder, x: f64, staff_y: f64, dir_words_y: f64, placement: Option<&str>, text: &str, line_idx: usize) {
    use super::lyrics::DIRECTION_WORDS_LINE_HEIGHT;
    let below = placement == Some("below");
    let line_offset = line_idx as f64 * DIRECTION_WORDS_LINE_HEIGHT;
    let y = if below {
        dir_words_y + line_offset
    } else {
        staff_y + CHORD_SYMBOL_OFFSET_Y - 14.0 - line_offset
    };
    svg.elements.push(format!(
        r#"<text x="{:.1}" y="{:.1}" font-family="Times New Roman, Times, serif" font-size="13" font-weight="bold" font-style="italic" fill="{}" text-anchor="end">{}</text>"#,
        x, y, NOTE_COLOR,
        text.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;")
    ));
}

pub(super) fn render_direction_words(svg: &mut SvgBuilder, x: f64, staff_y: f64, dir_words_y: f64, dir: &Direction, line_idx: usize) {
    use super::lyrics::DIRECTION_WORDS_LINE_HEIGHT;
    if let Some(ref text) = dir.words {
        if text.is_empty() { return; }
        let below = dir.placement.as_deref() == Some("below");
        let line_offset = line_idx as f64 * DIRECTION_WORDS_LINE_HEIGHT;
        let y = if below {
            dir_words_y + line_offset
        } else {
            staff_y + CHORD_SYMBOL_OFFSET_Y - 14.0 - line_offset
        };

        let (weight, style) = match dir.words_font_style.as_deref() {
            Some("bold italic") => ("bold", "italic"),
            Some("bold") => ("bold", "normal"),
            Some("italic") => ("normal", "italic"),
            _ => ("normal", "normal"),
        };

        svg.elements.push(format!(
            r#"<text x="{:.1}" y="{:.1}" font-family="Times New Roman, Times, serif" font-size="12" font-weight="{}" font-style="{}" fill="{}" text-anchor="start">{}</text>"#,
            x + 4.0, y, weight, style, NOTE_COLOR,
            text.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;")
        ));
    }
}

// ═══════════════════════════════════════════════════════════════════════
// Time signature rendering
// ═══════════════════════════════════════════════════════════════════════

pub(super) fn render_time_signature(svg: &mut SvgBuilder, x: f64, staff_y: f64, time: &TimeSignature) {
    let s = TIMESIG_GLYPH_SCALE;
    let top_y = staff_y + 2.0 * STAFF_LINE_SPACING;
    let bot_y = staff_y + 4.0 * STAFF_LINE_SPACING;

    let top_width = timesig_number_width(time.beats);
    let bot_width = timesig_number_width(time.beat_type);
    let max_width = top_width.max(bot_width);

    let top_start_x = x + (max_width - top_width) / 2.0;
    render_timesig_number(svg, top_start_x, top_y, time.beats, s);

    let bot_start_x = x + (max_width - bot_width) / 2.0;
    render_timesig_number(svg, bot_start_x, bot_y, time.beat_type, s);
}

fn render_timesig_number(svg: &mut SvgBuilder, mut x: f64, y: f64, n: i32, scale: f64) {
    let digits: Vec<u32> = if n == 0 {
        vec![0]
    } else {
        let mut d = Vec::new();
        let mut v = n.unsigned_abs();
        while v > 0 {
            d.push(v % 10);
            v /= 10;
        }
        d.reverse();
        d
    };

    for &d in &digits {
        let outline = timesig_digit_glyph(d);
        let path = vexflow_outline_to_svg(outline, scale, x, y);
        svg.path(&path, NOTE_COLOR, "none", 0.0);
        x += TIMESIG_DIGIT_HA[d as usize] * scale;
    }
}

// ═══════════════════════════════════════════════════════════════════════
// Harmony (chord symbol) rendering
// ═══════════════════════════════════════════════════════════════════════

pub(super) fn render_harmonies(
    svg: &mut SvgBuilder, measure: &Measure,
    mx: f64, mw: f64, staff_y: f64,
) {
    if measure.harmonies.is_empty() {
        return;
    }

    let spacing = mw / (measure.harmonies.len() as f64 + 1.0);

    for (i, harmony) in measure.harmonies.iter().enumerate() {
        let x = mx + spacing * (i as f64 + 0.5);
        let y = staff_y + CHORD_SYMBOL_OFFSET_Y;

        let kind_str = match harmony.kind.as_str() {
            "major" => "",
            "minor" => "m",
            "dominant" => "7",
            "dominant-seventh" => "7",
            "major-seventh" => "maj7",
            "minor-seventh" => "m7",
            "diminished" => "dim",
            "augmented" => "aug",
            "half-diminished" => "m7b5",
            other => other,
        };

        let alter_str = match harmony.root.alter {
            Some(a) if a > 0.0 => "#",
            Some(a) if a < 0.0 => "b",
            _ => "",
        };

        let label = format!("{}{}{}", harmony.root.step, alter_str, kind_str);
        svg.chord_text(x, y, &label, 12.0, CHORD_COLOR);
    }
}

// ═══════════════════════════════════════════════════════════════════════
// Barline rendering
// ═══════════════════════════════════════════════════════════════════════

pub(super) fn render_barlines(
    svg: &mut SvgBuilder, measure: &Measure,
    mx: f64, mw: f64, staff_y: f64,
) {
    for barline in &measure.barlines {
        let bx = match barline.location.as_str() {
            "left" => mx,
            "right" => mx + mw,
            _ => mx + mw,
        };

        match barline.bar_style.as_deref() {
            Some("heavy-light") => {
                svg.line(bx, staff_y, bx, staff_y + STAFF_HEIGHT, BARLINE_COLOR, 3.0);
                svg.line(bx + 5.0, staff_y, bx + 5.0, staff_y + STAFF_HEIGHT, BARLINE_COLOR, BARLINE_WIDTH);
                svg.circle(bx + 10.0, staff_y + 15.0, 2.0, BARLINE_COLOR);
                svg.circle(bx + 10.0, staff_y + 25.0, 2.0, BARLINE_COLOR);
            }
            Some("light-heavy") => {
                svg.circle(bx - 10.0, staff_y + 15.0, 2.0, BARLINE_COLOR);
                svg.circle(bx - 10.0, staff_y + 25.0, 2.0, BARLINE_COLOR);
                svg.line(bx - 5.0, staff_y, bx - 5.0, staff_y + STAFF_HEIGHT, BARLINE_COLOR, BARLINE_WIDTH);
                svg.line(bx, staff_y, bx, staff_y + STAFF_HEIGHT, BARLINE_COLOR, 3.0);
            }
            Some("light-light") => {
                svg.line(bx - 2.0, staff_y, bx - 2.0, staff_y + STAFF_HEIGHT, BARLINE_COLOR, BARLINE_WIDTH);
                svg.line(bx + 2.0, staff_y, bx + 2.0, staff_y + STAFF_HEIGHT, BARLINE_COLOR, BARLINE_WIDTH);
            }
            _ => {
                if let Some(ref repeat) = barline.repeat {
                    match repeat.direction.as_str() {
                        "forward" => {
                            svg.line(bx, staff_y, bx, staff_y + STAFF_HEIGHT, BARLINE_COLOR, 3.0);
                            svg.line(bx + 5.0, staff_y, bx + 5.0, staff_y + STAFF_HEIGHT, BARLINE_COLOR, BARLINE_WIDTH);
                            svg.circle(bx + 10.0, staff_y + 15.0, 2.0, BARLINE_COLOR);
                            svg.circle(bx + 10.0, staff_y + 25.0, 2.0, BARLINE_COLOR);
                        }
                        "backward" => {
                            svg.circle(bx - 10.0, staff_y + 15.0, 2.0, BARLINE_COLOR);
                            svg.circle(bx - 10.0, staff_y + 25.0, 2.0, BARLINE_COLOR);
                            svg.line(bx - 5.0, staff_y, bx - 5.0, staff_y + STAFF_HEIGHT, BARLINE_COLOR, BARLINE_WIDTH);
                            svg.line(bx, staff_y, bx, staff_y + STAFF_HEIGHT, BARLINE_COLOR, 3.0);
                        }
                        _ => {}
                    }
                }
            }
        }

        if let Some(ref ending) = barline.ending {
            if ending.ending_type == "start" {
                let text = ending.text.as_deref()
                    .unwrap_or(&ending.number);
                svg.text(bx + 5.0, staff_y - 10.0,
                         &format!("{}.", text), 10.0, "normal", BARLINE_COLOR, "start");
                svg.line(bx, staff_y - 5.0, bx, staff_y - 15.0, BARLINE_COLOR, BARLINE_WIDTH);
                svg.line(bx, staff_y - 15.0, bx + mw, staff_y - 15.0, BARLINE_COLOR, BARLINE_WIDTH);
            }
        }
    }
}
