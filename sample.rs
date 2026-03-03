// jianpu_svg.rs

// =======================
// Core data structures
// =======================

#[derive(Debug)]
pub struct Score {
    pub parts: Vec<Part>,
}

#[derive(Debug)]
pub struct Part {
    pub id: usize,
    pub measures: Vec<Measure>,
}

#[derive(Debug)]
pub struct Measure {
    pub index: usize,
    pub time_signature: TimeSignature,
    pub key: KeySignature,          // written key at this measure
    pub events: Vec<NoteEvent>,     // unrolled, in time order
}

#[derive(Debug, Clone)]
pub struct TimeSignature {
    pub numerator: u8,
    pub denominator: u8,
}

#[derive(Debug, Clone)]
pub struct NoteEvent {
    pub event_id: usize,
    pub start_beat: f32,
    pub duration_beats: f32,
    pub pitches: Vec<Pitch>,
    pub is_rest: bool,
    pub voice: u8,
    pub lyrics: Vec<Lyric>,         // multiple verses, full model
}

#[derive(Debug, Clone)]
pub struct Pitch {
    pub midi: u8,             // written pitch after all transpositions
    pub accidental: Accidental,
    pub octave_offset: i8,
}

#[derive(Debug, Clone)]
pub enum Accidental {
    None,
    Sharp,
    Flat,
    Natural,
}

#[derive(Debug, Clone, Copy)]
pub enum Mode {
    Major,
    Minor,
    Dorian,
    Mixolydian,
}

#[derive(Debug, Clone)]
pub struct KeySignature {
    pub tonic_pc: u8, // 0=C, 1=C#, ..., 11=B (written key)
    pub mode: Mode,
}

// Full lyric model (Option C)
#[derive(Debug, Clone)]
pub struct Lyric {
    pub number: u8,        // verse number (1-based)
    pub text: String,
    pub syllabic: Syllabic,
    pub elision: bool,     // for "gloria in excelsis" style
}

#[derive(Debug, Clone)]
pub enum Syllabic {
    Single,
    Begin,
    Middle,
    End,
}

#[derive(Debug)]
pub struct JianpuSettings {
    pub part_number: usize,
}

#[derive(Debug, Clone)]
pub struct LayoutBox {
    pub event_id: usize,
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
}

// =======================
// Simple layout cache hook
// =======================

#[derive(Debug, Default)]
pub struct JianpuLayoutCache {
    pub enabled: bool,
}

// =======================
// SVG builder helper
// =======================

pub struct SvgBuilder {
    buf: String,
}

impl SvgBuilder {
    pub fn new(width: f32, height: f32) -> Self {
        let mut buf = String::new();
        buf.push_str(&format!(
            r#"<svg xmlns="http://www.w3.org/2000/svg" width="{w}" height="{h}" viewBox="0 0 {w} {h}">"#,
            w = width,
            h = height
        ));
        Self { buf }
    }

    pub fn group_start(&mut self, transform: &str) {
        self.buf.push_str(&format!(r#"<g transform="{}">"#, transform));
    }

    pub fn group_end(&mut self) {
        self.buf.push_str("</g>");
    }

    pub fn text_center(&mut self, x: f32, y: f32, text: &str, font_size: f32) {
        self.buf.push_str(&format!(
            r#"<text x="{x}" y="{y}" font-size="{fs}" text-anchor="middle" dominant-baseline="middle">{t}</text>"#,
            x = x,
            y = y,
            fs = font_size,
            t = text
        ));
    }

    pub fn text_left(&mut self, x: f32, y: f32, text: &str, font_size: f32) {
        self.buf.push_str(&format!(
            r#"<text x="{x}" y="{y}" font-size="{fs}" text-anchor="start" dominant-baseline="middle">{t}</text>"#,
            x = x,
            y = y,
            fs = font_size,
            t = text
        ));
    }

    pub fn line(&mut self, x1: f32, y1: f32, x2: f32, y2: f32, stroke_width: f32) {
        self.buf.push_str(&format!(
            r#"<line x1="{x1}" y1="{y1}" x2="{x2}" y2="{y2}" stroke="black" stroke-width="{sw}"/>"#,
            x1 = x1,
            y1 = y1,
            x2 = x2,
            y2 = y2,
            sw = stroke_width
        ));
    }

    pub fn circle(&mut self, cx: f32, cy: f32, r: f32) {
        self.buf.push_str(&format!(
            r#"<circle cx="{cx}" cy="{cy}" r="{r}" fill="black"/>"#,
            cx = cx,
            cy = cy,
            r = r
        ));
    }

    pub fn finish(mut self) -> String {
        self.buf.push_str("</svg>");
        self.buf
    }
}

// =======================
// Global engraving metrics
// =======================

#[derive(Debug, Clone, Copy)]
struct Metrics {
    U: f32,
    num_height: f32,
    num_width: f32,
    dot_radius: f32,
    dot_offset_first: f32,
    dot_spacing: f32,
    underline_thickness: f32,
    underline_base_offset: f32,
    underline_spacing: f32,
    accidental_width: f32,
    accidental_gap: f32,
    key_label_font: f32,
    number_font: f32,
    lyric_font: f32,
    lyric_line_spacing: f32,
}

fn default_metrics() -> Metrics {
    let U = 10.0;
    Metrics {
        U,
        num_height: U,
        num_width: 0.8 * U,
        dot_radius: 0.15 * U,
        dot_offset_first: 0.3 * U,
        dot_spacing: 0.5 * U,
        underline_thickness: 0.08 * U,
        underline_base_offset: 0.3 * U,
        underline_spacing: 0.4 * U,
        accidental_width: 0.6 * U,
        accidental_gap: 0.15 * U,
        key_label_font: 1.2 * U,
        number_font: U,
        lyric_font: 0.9 * U,
        lyric_line_spacing: 1.1 * U,
    }
}

// =======================
// Public entry point
// =======================

pub fn render_jianpu_svg(
    score: &Score,
    settings: &JianpuSettings,
    _cache: &mut JianpuLayoutCache,
) -> (String, Vec<LayoutBox>) {
    let part_index = if settings.part_number == 0 || settings.part_number > score.parts.len() {
        0
    } else {
        settings.part_number - 1
    };

    let part = &score.parts[part_index];

    let width = 1200.0;
    let height = 800.0;
    let system_height = 80.0;
    let system_spacing = 40.0;
    let metrics = default_metrics();

    let mut svg = SvgBuilder::new(width, height);
    let mut layout_boxes: Vec<LayoutBox> = Vec::new();

    for (i, measure) in part.measures.iter().enumerate() {
        let system_y = 80.0 + i as f32 * (system_height + system_spacing);
        let x_start = 80.0;
        let usable_width = width - 160.0;

        render_measure_jianpu(
            &mut svg,
            measure,
            &metrics,
            x_start,
            system_y,
            usable_width,
            &mut layout_boxes,
            i == 0,
        );
    }

    (svg.finish(), layout_boxes)
}

// =======================
// Measure rendering
// =======================

#[derive(Clone, Debug)]
struct UnderlineSpan {
    level: u8,
    x_start: f32,
    x_end: f32,
    voice: u8,
}

fn render_measure_jianpu(
    svg: &mut SvgBuilder,
    measure: &Measure,
    m: &Metrics,
    x_start: f32,
    y_center: f32,
    width: f32,
    layout_boxes: &mut Vec<LayoutBox>,
    draw_key_label_here: bool,
) {
    let events = &measure.events;
    if events.is_empty() {
        return;
    }

    let total_beats = measure.time_signature.numerator as f32;

    let mut note_positions: Vec<f32> = Vec::with_capacity(events.len());
    for ev in events {
        let rel = if total_beats > 0.0 {
            ev.start_beat / total_beats
        } else {
            0.0
        };
        let x = x_start + rel * width;
        note_positions.push(x);
    }

    if draw_key_label_here {
        let label = key_label_text(&measure.key);
        let label_x = x_start;
        let label_y = y_center - 2.0 * m.U;
        svg.text_left(label_x, label_y, &label, m.key_label_font);
    }

    let underline_spans =
        compute_underline_spans(events, &note_positions, &measure.time_signature);
    let max_underline_levels = max_underline_levels(events, &measure.time_signature);

    draw_underline_spans(svg, &underline_spans, m, y_center);

    for (idx, ev) in events.iter().enumerate() {
        if ev.is_rest {
            continue;
        }
        let x = note_positions[idx];

        let bbox = render_note_event(svg, ev, m, x, y_center, &measure.key);
        layout_boxes.push(LayoutBox {
            event_id: ev.event_id,
            x: bbox.0,
            y: bbox.1,
            width: bbox.2,
            height: bbox.3,
        });

        if !ev.lyrics.is_empty() {
            let base_lyric_y = y_center
                + m.underline_base_offset
                + m.underline_spacing * max_underline_levels as f32
                + m.U * 1.2;

            for lyric in &ev.lyrics {
                let line_index = (lyric.number.saturating_sub(1)) as f32;
                let lyric_y = base_lyric_y + line_index * m.lyric_line_spacing;

                let (lx, ly, lw, lh) =
                    render_single_lyric(svg, lyric, x, lyric_y, m.lyric_font);

                layout_boxes.push(LayoutBox {
                    event_id: ev.event_id,
                    x: lx,
                    y: ly,
                    width: lw,
                    height: lh,
                });
            }
        }
    }

    svg.line(x_start, y_center, x_start + width, y_center, 0.5);
}

// =======================
// Key label
// =======================

fn key_label_text(key: &KeySignature) -> String {
    let tonic_name = pitch_class_to_name(key.tonic_pc);
    let mode_suffix = match key.mode {
        Mode::Major => "",
        Mode::Minor => "m",
        Mode::Dorian => " (Dorian)",
        Mode::Mixolydian => " (Mixolydian)",
    };
    format!("1 = {}{}", tonic_name, mode_suffix)
}

fn pitch_class_to_name(pc: u8) -> &'static str {
    match pc % 12 {
        0 => "C",
        1 => "C#",
        2 => "D",
        3 => "Eb",
        4 => "E",
        5 => "F",
        6 => "F#",
        7 => "G",
        8 => "Ab",
        9 => "A",
        10 => "Bb",
        11 => "B",
        _ => "?",
    }
}

// =======================
// Underline / beam logic
// =======================

fn duration_to_underline_count(duration_beats: f32, _ts: &TimeSignature) -> u8 {
    let quarter = 1.0;
    let eighth = quarter / 2.0;
    let sixteenth = quarter / 4.0;
    let thirty_second = quarter / 8.0;

    if (duration_beats - eighth).abs() < 1e-6 {
        1
    } else if (duration_beats - sixteenth).abs() < 1e-6 {
        2
    } else if (duration_beats - thirty_second).abs() < 1e-6 {
        3
    } else {
        0
    }
}

fn compute_underline_spans(
    events: &[NoteEvent],
    xs: &[f32],
    ts: &TimeSignature,
) -> Vec<UnderlineSpan> {
    use std::collections::HashMap;

    let mut spans = Vec::new();
    let mut by_voice: HashMap<u8, Vec<(usize, &NoteEvent)>> = HashMap::new();

    for (i, ev) in events.iter().enumerate() {
        if ev.is_rest {
            continue;
        }
        by_voice.entry(ev.voice).or_default().push((i, ev));
    }

    for (voice, list) in by_voice {
        if list.is_empty() {
            continue;
        }

        let max_level = list
            .iter()
            .map(|(_, ev)| duration_to_underline_count(ev.duration_beats, ts))
            .max()
            .unwrap_or(0);

        for level in 1..=max_level {
            let mut current_start: Option<usize> = None;
            let mut last_idx: Option<usize> = None;

            for (idx, ev) in &list {
                let count = duration_to_underline_count(ev.duration_beats, ts);
                if count >= level {
                    if current_start.is_none() {
                        current_start = Some(*idx);
                    }
                    last_idx = Some(*idx);
                } else {
                    if let (Some(s), Some(e)) = (current_start, last_idx) {
                        spans.push(UnderlineSpan {
                            level,
                            x_start: xs[s],
                            x_end: xs[e],
                            voice,
                        });
                    }
                    current_start = None;
                    last_idx = None;
                }
            }

            if let (Some(s), Some(e)) = (current_start, last_idx) {
                spans.push(UnderlineSpan {
                    level,
                    x_start: xs[s],
                    x_end: xs[e],
                    voice,
                });
            }
        }
    }

    spans
}

fn max_underline_levels(events: &[NoteEvent], ts: &TimeSignature) -> u8 {
    events
        .iter()
        .filter(|e| !e.is_rest)
        .map(|e| duration_to_underline_count(e.duration_beats, ts))
        .max()
        .unwrap_or(0)
}

fn draw_underline_spans(svg: &mut SvgBuilder, spans: &[UnderlineSpan], m: &Metrics, y_center: f32) {
    for span in spans {
        let y = y_center + m.underline_base_offset + (span.level as f32 - 1.0) * m.underline_spacing;
        let x1 = span.x_start - 0.6 * m.U;
        let x2 = span.x_end + 0.6 * m.U;
        svg.line(x1, y, x2, y, m.underline_thickness);
    }
}

// =======================
// Per-note Jianpu glyphs
// =======================

fn render_note_event(
    svg: &mut SvgBuilder,
    ev: &NoteEvent,
    m: &Metrics,
    x: f32,
    y_center: f32,
    key: &KeySignature,
) -> (f32, f32, f32, f32) {
    let n = ev.pitches.len();
    if n == 0 {
        return (x, y_center, 0.0, 0.0);
    }

    let h_num = m.num_height;
    let w_num = m.num_width;
    let spacing = 0.9 * h_num;

    let mut pitches = ev.pitches.clone();
    pitches.sort_by_key(|p| p.midi);

    let mut min_y = f32::MAX;
    let mut max_y = f32::MIN;

    svg.group_start(&format!("translate({x},{y})", x = x, y = y_center));

    for (i, pitch) in pitches.iter().enumerate() {
        let offset = i as f32 - (n as f32 - 1.0) / 2.0;
        let y_local = offset * spacing;

        let (local_min_y, local_max_y) =
            render_single_jianpu_glyph(svg, pitch, m, w_num, h_num, 0.0, y_local, key);

        min_y = min_y.min(local_min_y);
        max_y = max_y.max(local_max_y);
    }

    svg.group_end();

    let width = w_num * 2.0;
    let height = max_y - min_y;
    let x_left = x - width / 2.0;
    let y_top = y_center + min_y;

    (x_left, y_top, width, height)
}

fn render_single_jianpu_glyph(
    svg: &mut SvgBuilder,
    pitch: &Pitch,
    m: &Metrics,
    w_num: f32,
    h_num: f32,
    x_center: f32,
    y_center: f32,
    key: &KeySignature,
) -> (f32, f32) {
    let font_size = m.number_font;

    let number_text = scale_degree_for_pitch(pitch.midi, key);

    svg.text_center(x_center, y_center, &number_text, font_size);

    let mut min_y = y_center - h_num / 2.0;
    let mut max_y = y_center + h_num / 2.0;

    let top = y_center - h_num / 2.0;
    let bottom = y_center + h_num / 2.0;

    if pitch.octave_offset > 0 {
        for i in 0..pitch.octave_offset {
            let dy = top - (m.dot_offset_first + i as f32 * m.dot_spacing);
            svg.circle(x_center, dy, m.dot_radius);
            min_y = min_y.min(dy - m.dot_radius);
        }
    } else if pitch.octave_offset < 0 {
        for i in 0..(-pitch.octave_offset) {
            let dy = bottom + (m.dot_offset_first + i as f32 * m.dot_spacing);
            svg.circle(x_center, dy, m.dot_radius);
            max_y = max_y.max(dy + m.dot_radius);
        }
    }

    if let Accidental::None = pitch.accidental {
    } else {
        let acc_x = x_center - w_num / 2.0 - m.accidental_gap - m.accidental_width / 2.0;
        let acc_y = y_center;
        let acc_text = match pitch.accidental {
            Accidental::Sharp => "#",
            Accidental::Flat => "b",
            Accidental::Natural => "♮",
            Accidental::None => "",
        };
        svg.text_center(acc_x, acc_y, acc_text, font_size * 0.9);
    }

    (min_y, max_y)
}

// =======================
// Lyrics rendering
// =======================

fn render_single_lyric(
    svg: &mut SvgBuilder,
    lyric: &Lyric,
    x: f32,
    y: f32,
    font_size: f32,
) -> (f32, f32, f32, f32) {
    let mut text = lyric.text.clone();

    match lyric.syllabic {
        Syllabic::Begin | Syllabic::Middle => text.push('-'),
        Syllabic::Single | Syllabic::End => {}
    }

    if lyric.elision {
        text.push(' ');
        text.push('~');
    }

    svg.text_center(x, y, &text, font_size);

    let height = font_size;
    let min_y = y - height * 0.5;
    let max_y = y + height * 0.5;
    let width = text_width_estimate(&text, font_size);

    (x - width / 2.0, min_y, width, max_y - min_y)
}

fn text_width_estimate(text: &str, font_size: f32) -> f32 {
    let avg_char_width = font_size * 0.6;
    text.chars().count() as f32 * avg_char_width
}

// =======================
// SCALE DEGREE MAPPING
// =======================

fn pitch_class(midi: u8) -> u8 {
    midi % 12
}

fn build_scale_for_mode(tonic_pc: u8, mode: Mode) -> [u8; 7] {
    let intervals = match mode {
        Mode::Major => [0, 2, 4, 5, 7, 9, 11],
        Mode::Minor => [0, 2, 3, 5, 7, 8, 10],
        Mode::Dorian => [0, 2, 3, 5, 7, 9, 10],
        Mode::Mixolydian => [0, 2, 4, 5, 7, 9, 10],
    };

    let mut scale = [0u8; 7];
    for i in 0..7 {
        scale[i] = (tonic_pc + intervals[i]) % 12;
    }
    scale
}

fn find_scale_degree(note_pc: u8, scale: &[u8; 7]) -> (usize, i8) {
    let mut best_degree: usize = 0;
    let mut best_offset: i8 = 127; // large sentinel

    for d in 0..7 {
        let target_pc = scale[d] as i8;
        let note_pc_i = note_pc as i8;

        let diff_raw = note_pc_i - target_pc;
        let diff_mod = ((diff_raw % 12) + 12) % 12; // 0..11
        let diff_mod = diff_mod as i8;

        let offset = if diff_mod <= 6 { diff_mod } else { diff_mod - 12 };

        if offset.abs() < best_offset.abs() {
            best_offset = offset;
            best_degree = d;
        }
    }

    (best_degree, best_offset)
}

fn accidental_symbol(offset: i8) -> &'static str {
    match offset {
        0 => "",
        1 => "#",
        -1 => "b",
        2 => "##",
        -2 => "bb",
        _ => "?",
    }
}

pub fn scale_degree_for_pitch(written_midi: u8, key: &KeySignature) -> String {
    let note_pc = pitch_class(written_midi);
    let scale = build_scale_for_mode(key.tonic_pc, key.mode);

    let (degree_index, accidental_offset) = find_scale_degree(note_pc, &scale);

    let accidental = accidental_symbol(accidental_offset);
    let degree_number = (degree_index + 1).to_string();

    format!("{}{}", accidental, degree_number)
}
