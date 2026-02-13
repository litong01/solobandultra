//! MusicXML parser — converts MusicXML XML into the Score data model.

use roxmltree::{Document, Node};

use crate::model::*;

/// Parse a MusicXML XML string into a Score.
pub fn parse_musicxml(xml: &str) -> Result<Score, String> {
    // MusicXML files include a DOCTYPE declaration, so we must allow DTDs
    let options = roxmltree::ParsingOptions {
        allow_dtd: true,
        ..Default::default()
    };
    let doc = Document::parse_with_options(xml, options)
        .map_err(|e| format!("XML parse error: {e}"))?;
    let root = doc.root_element();

    // Verify this is a score-partwise document
    if root.tag_name().name() != "score-partwise" {
        return Err(format!(
            "Unsupported root element: '{}'. Only 'score-partwise' is supported.",
            root.tag_name().name()
        ));
    }

    let mut score = Score::new();
    score.version = root.attribute("version").map(String::from);

    // Parse top-level elements
    for child in root.children().filter(|n| n.is_element()) {
        match child.tag_name().name() {
            "work" => parse_work(&child, &mut score),
            "identification" => parse_identification(&child, &mut score),
            "defaults" => score.defaults = Some(parse_defaults(&child)),
            "credit" => parse_credit(&child, &mut score),
            "part-list" => parse_part_list(&child, &mut score),
            "part" => parse_part(&child, &mut score),
            _ => {}
        }
    }

    Ok(score)
}

// ─── Work ────────────────────────────────────────────────────────────

fn parse_work(node: &Node, score: &mut Score) {
    for child in node.children().filter(|n| n.is_element()) {
        if child.tag_name().name() == "work-title" {
            // Only use work-title as a fallback; <credit type="title"> takes priority.
            if score.title.is_none() {
                score.title = child.text().map(|t| t.trim().to_string());
            }
        }
    }
}

// ─── Identification ──────────────────────────────────────────────────

fn parse_identification(node: &Node, score: &mut Score) {
    for child in node.children().filter(|n| n.is_element()) {
        match child.tag_name().name() {
            "creator" => {
                let creator_type = child.attribute("type").unwrap_or("");
                let text = child.text().map(|t| t.trim().to_string());
                match creator_type {
                    // Only use <creator type="composer"> as a fallback;
                    // <credit type="composer"> takes priority.
                    "composer" => {
                        if score.composer.is_none() {
                            score.composer = text;
                        }
                    }
                    "arranger" => score.arranger = text,
                    _ => {}
                }
            }
            "encoding" => {
                for enc_child in child.children().filter(|n| n.is_element()) {
                    if enc_child.tag_name().name() == "software" {
                        score.software = enc_child.text().map(|t| t.trim().to_string());
                    }
                }
            }
            _ => {}
        }
    }
}

// ─── Defaults ────────────────────────────────────────────────────────

fn parse_defaults(node: &Node) -> Defaults {
    let mut defaults = Defaults {
        millimeters: None,
        tenths: None,
        page_height: None,
        page_width: None,
        left_margin: None,
        right_margin: None,
        top_margin: None,
        bottom_margin: None,
    };

    for child in node.children().filter(|n| n.is_element()) {
        match child.tag_name().name() {
            "scaling" => {
                for sc in child.children().filter(|n| n.is_element()) {
                    match sc.tag_name().name() {
                        "millimeters" => defaults.millimeters = parse_f64(&sc),
                        "tenths" => defaults.tenths = parse_f64(&sc),
                        _ => {}
                    }
                }
            }
            "page-layout" => {
                for pl in child.children().filter(|n| n.is_element()) {
                    match pl.tag_name().name() {
                        "page-height" => defaults.page_height = parse_f64(&pl),
                        "page-width" => defaults.page_width = parse_f64(&pl),
                        "page-margins" => {
                            for pm in pl.children().filter(|n| n.is_element()) {
                                match pm.tag_name().name() {
                                    "left-margin" => defaults.left_margin = parse_f64(&pm),
                                    "right-margin" => defaults.right_margin = parse_f64(&pm),
                                    "top-margin" => defaults.top_margin = parse_f64(&pm),
                                    "bottom-margin" => defaults.bottom_margin = parse_f64(&pm),
                                    _ => {}
                                }
                            }
                        }
                        _ => {}
                    }
                }
            }
            _ => {}
        }
    }

    defaults
}

// ─── Credits ─────────────────────────────────────────────────────────

fn parse_credit(node: &Node, score: &mut Score) {
    let mut credit_type = String::new();
    let mut credit_text = String::new();
    let mut style = TextStyle::default();

    for child in node.children().filter(|n| n.is_element()) {
        match child.tag_name().name() {
            "credit-type" => {
                credit_type = child.text().unwrap_or("").trim().to_string();
            }
            "credit-words" => {
                let text = child.text().unwrap_or("").trim();
                if !text.is_empty() {
                    if !credit_text.is_empty() {
                        credit_text.push('\n');
                    }
                    credit_text.push_str(text);
                }
                // Extract font attributes from the first <credit-words> element
                // (the primary one that typically carries the style).
                if style.font_family.is_none() {
                    style.font_family = child.attribute("font-family")
                        .map(|s| s.trim().to_string())
                        .filter(|s| !s.is_empty());
                }
                if style.font_size.is_none() {
                    style.font_size = child.attribute("font-size")
                        .and_then(|s| s.trim().parse::<f64>().ok());
                }
                if style.font_weight.is_none() {
                    style.font_weight = child.attribute("font-weight")
                        .map(|s| s.trim().to_string())
                        .filter(|s| !s.is_empty());
                }
                if style.font_style.is_none() {
                    style.font_style = child.attribute("font-style")
                        .map(|s| s.trim().to_string())
                        .filter(|s| !s.is_empty());
                }
            }
            _ => {}
        }
    }

    let has_style = style.font_family.is_some()
        || style.font_size.is_some()
        || style.font_weight.is_some()
        || style.font_style.is_some();

    match credit_type.as_str() {
        // <credit> values are the primary source for title and composer;
        // <work-title> and <creator type="composer"> are fallbacks.
        "title" => {
            if !credit_text.is_empty() {
                score.title = Some(credit_text);
                if has_style {
                    score.title_style = Some(style);
                }
            }
        }
        "subtitle" => {
            score.subtitle = Some(credit_text);
            if has_style {
                score.subtitle_style = Some(style);
            }
        }
        "composer" => {
            if !credit_text.is_empty() {
                score.composer = Some(credit_text);
                if has_style {
                    score.composer_style = Some(style);
                }
            }
        }
        _ => {}
    }
}

// ─── Part List ───────────────────────────────────────────────────────

fn parse_part_list(node: &Node, score: &mut Score) {
    for child in node.children().filter(|n| n.is_element()) {
        if child.tag_name().name() == "score-part" {
            let id = child.attribute("id").unwrap_or("").to_string();
            let mut part = Part {
                id,
                name: String::new(),
                abbreviation: None,
                midi_program: None,
                midi_channel: None,
                measures: Vec::new(),
            };

            for sp_child in child.children().filter(|n| n.is_element()) {
                match sp_child.tag_name().name() {
                    "part-name" => {
                        part.name = sp_child.text().unwrap_or("").trim().to_string();
                    }
                    "part-abbreviation" => {
                        part.abbreviation =
                            sp_child.text().map(|t| t.trim().to_string());
                    }
                    "midi-instrument" => {
                        for midi in sp_child.children().filter(|n| n.is_element()) {
                            match midi.tag_name().name() {
                                "midi-channel" => {
                                    part.midi_channel = parse_i32(&midi);
                                }
                                "midi-program" => {
                                    part.midi_program = parse_i32(&midi);
                                }
                                _ => {}
                            }
                        }
                    }
                    _ => {}
                }
            }

            score.parts.push(part);
        }
    }
}

// ─── Part (measures) ─────────────────────────────────────────────────

fn parse_part(node: &Node, score: &mut Score) {
    let part_id = node.attribute("id").unwrap_or("").to_string();

    // Find the matching part from the part-list
    let part = match score.parts.iter_mut().find(|p| p.id == part_id) {
        Some(p) => p,
        None => return,
    };

    for child in node.children().filter(|n| n.is_element()) {
        if child.tag_name().name() == "measure" {
            part.measures.push(parse_measure(&child));
        }
    }
}

// ─── Measure ─────────────────────────────────────────────────────────

fn parse_measure(node: &Node) -> Measure {
    let number = node
        .attribute("number")
        .and_then(|n| n.parse::<i32>().ok())
        .unwrap_or(0);
    let implicit = node.attribute("implicit") == Some("yes");
    let width = node
        .attribute("width")
        .and_then(|w| w.parse::<f64>().ok());

    let mut measure = Measure {
        number,
        implicit,
        width,
        attributes: None,
        notes: Vec::new(),
        harmonies: Vec::new(),
        barlines: Vec::new(),
        directions: Vec::new(),
        new_system: false,
        new_page: false,
    };

    for child in node.children().filter(|n| n.is_element()) {
        match child.tag_name().name() {
            "attributes" => measure.attributes = Some(parse_attributes(&child)),
            "note" => measure.notes.push(parse_note(&child)),
            "harmony" => measure.harmonies.push(parse_harmony(&child)),
            "barline" => measure.barlines.push(parse_barline(&child)),
            "direction" => {
                if let Some(dir) = parse_direction(&child) {
                    measure.directions.push(dir);
                }
            }
            "sound" => {
                // <sound> can appear directly in <measure> (not inside <direction>)
                if let Some(tempo) = child.attribute("tempo").and_then(|t| t.parse::<f64>().ok()) {
                    measure.directions.push(Direction {
                        placement: Some("above".to_string()),
                        sound_tempo: Some(tempo),
                        metronome: None,
                        words: None,
                        segno: false,
                        coda: false,
                        rehearsal: None,
                        sound_dacapo: false,
                        sound_dalsegno: false,
                        sound_fine: false,
                        sound_tocoda: false,
                        words_font_style: None,
                        octave_shift_type: None,
                        octave_shift_size: 0,
                    });
                }
            }
            "print" => {
                if child.attribute("new-system") == Some("yes") {
                    measure.new_system = true;
                }
                if child.attribute("new-page") == Some("yes") {
                    measure.new_page = true;
                }
                // First measure's <print> without new-system attr also implies system start
                if child.children().any(|n| {
                    n.is_element() && n.tag_name().name() == "system-layout"
                }) {
                    measure.new_system = true;
                }
            }
            _ => {}
        }
    }

    measure
}

// ─── Attributes ──────────────────────────────────────────────────────

fn parse_attributes(node: &Node) -> Attributes {
    let mut attrs = Attributes {
        divisions: None,
        key: None,
        time: None,
        clefs: Vec::new(),
        transpose: None,
        staves: None,
    };

    for child in node.children().filter(|n| n.is_element()) {
        match child.tag_name().name() {
            "divisions" => attrs.divisions = parse_i32(&child),
            "key" => attrs.key = Some(parse_key(&child)),
            "time" => attrs.time = Some(parse_time(&child)),
            "staves" => attrs.staves = parse_i32(&child),
            "clef" => attrs.clefs.push(parse_clef(&child)),
            "transpose" => attrs.transpose = Some(parse_transpose(&child)),
            _ => {}
        }
    }

    attrs
}

fn parse_key(node: &Node) -> Key {
    let mut key = Key {
        fifths: 0,
        mode: None,
    };
    for child in node.children().filter(|n| n.is_element()) {
        match child.tag_name().name() {
            "fifths" => key.fifths = parse_i32(&child).unwrap_or(0),
            "mode" => key.mode = child.text().map(|t| t.trim().to_string()),
            _ => {}
        }
    }
    key
}

fn parse_time(node: &Node) -> TimeSignature {
    let mut ts = TimeSignature {
        beats: 4,
        beat_type: 4,
    };
    for child in node.children().filter(|n| n.is_element()) {
        match child.tag_name().name() {
            "beats" => ts.beats = parse_i32(&child).unwrap_or(4),
            "beat-type" => ts.beat_type = parse_i32(&child).unwrap_or(4),
            _ => {}
        }
    }
    ts
}

fn parse_clef(node: &Node) -> Clef {
    let number = node
        .attribute("number")
        .and_then(|n| n.parse::<i32>().ok())
        .unwrap_or(1);
    let mut clef = Clef {
        number,
        sign: "G".to_string(),
        line: 2,
        octave_change: None,
    };
    for child in node.children().filter(|n| n.is_element()) {
        match child.tag_name().name() {
            "sign" => {
                clef.sign = child.text().unwrap_or("G").trim().to_string();
            }
            "line" => clef.line = parse_i32(&child).unwrap_or(2),
            "clef-octave-change" => clef.octave_change = parse_i32(&child),
            _ => {}
        }
    }
    clef
}

fn parse_transpose(node: &Node) -> Transpose {
    let mut t = Transpose {
        diatonic: 0,
        chromatic: 0,
        octave_change: None,
    };
    for child in node.children().filter(|n| n.is_element()) {
        match child.tag_name().name() {
            "diatonic" => t.diatonic = parse_i32(&child).unwrap_or(0),
            "chromatic" => t.chromatic = parse_i32(&child).unwrap_or(0),
            "octave-change" => t.octave_change = parse_i32(&child),
            _ => {}
        }
    }
    t
}

// ─── Note ────────────────────────────────────────────────────────────

fn parse_note(node: &Node) -> Note {
    let mut note = Note {
        pitch: None,
        duration: 0,
        voice: None,
        note_type: None,
        stem: None,
        beams: Vec::new(),
        rest: false,
        measure_rest: false,
        chord: false,
        dot: false,
        accidental: None,
        tie_start: false,
        tie_stop: false,
        staff: None,
        default_x: node
            .attribute("default-x")
            .and_then(|v| v.parse().ok()),
        default_y: node
            .attribute("default-y")
            .and_then(|v| v.parse().ok()),
        lyrics: Vec::new(),
        grace: false,
        grace_slash: false,
        slurs: Vec::new(),
    };

    for child in node.children().filter(|n| n.is_element()) {
        match child.tag_name().name() {
            "pitch" => note.pitch = Some(parse_pitch(&child)),
            "duration" => note.duration = parse_i32(&child).unwrap_or(0),
            "voice" => note.voice = parse_i32(&child),
            "staff" => note.staff = parse_i32(&child),
            "type" => {
                note.note_type = child.text().map(|t| t.trim().to_string());
            }
            "stem" => {
                note.stem = child.text().map(|t| t.trim().to_string());
            }
            "beam" => {
                let number = child
                    .attribute("number")
                    .and_then(|n| n.parse().ok())
                    .unwrap_or(1);
                let beam_type = child.text().unwrap_or("").trim().to_string();
                note.beams.push(Beam { number, beam_type });
            }
            "rest" => {
                note.rest = true;
                if child.attribute("measure") == Some("yes") {
                    note.measure_rest = true;
                }
            }
            "grace" => {
                note.grace = true;
                if child.attribute("slash") == Some("yes") {
                    note.grace_slash = true;
                }
            }
            "chord" => note.chord = true,
            "dot" => note.dot = true,
            "accidental" => {
                note.accidental = child.text().map(|t| t.trim().to_string());
            }
            "tie" => {
                match child.attribute("type") {
                    Some("start") => note.tie_start = true,
                    Some("stop") => note.tie_stop = true,
                    _ => {}
                }
            }
            "notations" => {
                for nc in child.children().filter(|n| n.is_element()) {
                    if nc.tag_name().name() == "slur" {
                        let slur_type = nc.attribute("type").unwrap_or("").to_string();
                        let number = nc.attribute("number")
                            .and_then(|n| n.parse().ok())
                            .unwrap_or(1);
                        let placement = nc.attribute("placement").map(String::from);
                        note.slurs.push(SlurEvent { slur_type, number, placement });
                    }
                }
            }
            "lyric" => {
                let number = child
                    .attribute("number")
                    .and_then(|n| n.parse().ok())
                    .unwrap_or(1);
                let mut text = String::new();
                let mut syllabic = None;
                for lc in child.children().filter(|n| n.is_element()) {
                    match lc.tag_name().name() {
                        "text" => {
                            text = lc.text().unwrap_or("").trim().to_string();
                        }
                        "syllabic" => {
                            syllabic = lc.text().map(|t| t.trim().to_string());
                        }
                        _ => {}
                    }
                }
                if !text.is_empty() {
                    note.lyrics.push(Lyric { number, text, syllabic });
                }
            }
            _ => {}
        }
    }

    note
}

fn parse_pitch(node: &Node) -> Pitch {
    let mut pitch = Pitch {
        step: "C".to_string(),
        octave: 4,
        alter: None,
    };
    for child in node.children().filter(|n| n.is_element()) {
        match child.tag_name().name() {
            "step" => {
                pitch.step = child.text().unwrap_or("C").trim().to_string();
            }
            "octave" => pitch.octave = parse_i32(&child).unwrap_or(4),
            "alter" => pitch.alter = parse_f64(&child),
            _ => {}
        }
    }
    pitch
}

// ─── Harmony ─────────────────────────────────────────────────────────

fn parse_harmony(node: &Node) -> Harmony {
    let mut root = HarmonyRoot {
        step: "C".to_string(),
        alter: None,
    };
    let mut kind = "major".to_string();
    let mut bass = None;

    for child in node.children().filter(|n| n.is_element()) {
        match child.tag_name().name() {
            "root" => {
                for rc in child.children().filter(|n| n.is_element()) {
                    match rc.tag_name().name() {
                        "root-step" => {
                            root.step = rc.text().unwrap_or("C").trim().to_string();
                        }
                        "root-alter" => root.alter = parse_f64(&rc),
                        _ => {}
                    }
                }
            }
            "kind" => {
                kind = child.text().unwrap_or("major").trim().to_string();
            }
            "bass" => {
                let mut b = HarmonyRoot {
                    step: "C".to_string(),
                    alter: None,
                };
                for bc in child.children().filter(|n| n.is_element()) {
                    match bc.tag_name().name() {
                        "bass-step" => {
                            b.step = bc.text().unwrap_or("C").trim().to_string();
                        }
                        "bass-alter" => b.alter = parse_f64(&bc),
                        _ => {}
                    }
                }
                bass = Some(b);
            }
            _ => {}
        }
    }

    Harmony { root, kind, bass }
}

// ─── Barline ─────────────────────────────────────────────────────────

fn parse_barline(node: &Node) -> Barline {
    let location = node
        .attribute("location")
        .unwrap_or("right")
        .to_string();
    let mut barline = Barline {
        location,
        bar_style: None,
        repeat: None,
        ending: None,
    };

    for child in node.children().filter(|n| n.is_element()) {
        match child.tag_name().name() {
            "bar-style" => {
                barline.bar_style = child.text().map(|t| t.trim().to_string());
            }
            "repeat" => {
                let direction = child
                    .attribute("direction")
                    .unwrap_or("forward")
                    .to_string();
                barline.repeat = Some(Repeat { direction });
            }
            "ending" => {
                let number = child.attribute("number").unwrap_or("1").to_string();
                let ending_type = child.attribute("type").unwrap_or("start").to_string();
                let text = child.text().map(|t| t.trim().to_string());
                barline.ending = Some(Ending {
                    number,
                    ending_type,
                    text,
                });
            }
            _ => {}
        }
    }

    barline
}

// ─── Direction ───────────────────────────────────────────────────────

fn parse_direction(node: &Node) -> Option<Direction> {
    let placement = node.attribute("placement").map(String::from);

    let mut sound_tempo = None;
    let mut metronome = None;
    let mut words = None;
    let mut words_font_style = None;
    let mut segno = false;
    let mut coda = false;
    let mut rehearsal = None;
    let mut sound_dacapo = false;
    let mut sound_dalsegno = false;
    let mut sound_fine = false;
    let mut sound_tocoda = false;
    let mut octave_shift_type: Option<String> = None;
    let mut octave_shift_size: i32 = 0;

    for child in node.children().filter(|n| n.is_element()) {
        match child.tag_name().name() {
            "direction-type" => {
                for dt_child in child.children().filter(|n| n.is_element()) {
                    match dt_child.tag_name().name() {
                        "metronome" => {
                            metronome = Some(parse_metronome(&dt_child));
                        }
                        "words" => {
                            words = dt_child.text().map(|t| t.trim().to_string());
                            // Capture font style attributes
                            let bold = dt_child.attribute("font-weight") == Some("bold");
                            let italic = dt_child.attribute("font-style") == Some("italic");
                            words_font_style = match (bold, italic) {
                                (true, true) => Some("bold italic".to_string()),
                                (true, false) => Some("bold".to_string()),
                                (false, true) => Some("italic".to_string()),
                                _ => None,
                            };
                        }
                        "segno" => { segno = true; }
                        "coda" => { coda = true; }
                        "rehearsal" => {
                            rehearsal = dt_child.text().map(|t| t.trim().to_string());
                        }
                        "octave-shift" => {
                            octave_shift_type = dt_child.attribute("type").map(String::from);
                            octave_shift_size = dt_child.attribute("size")
                                .and_then(|s| s.parse::<i32>().ok())
                                .unwrap_or(8);
                        }
                        _ => {}
                    }
                }
            }
            "sound" => {
                if let Some(tempo) = child.attribute("tempo").and_then(|t| t.parse::<f64>().ok()) {
                    sound_tempo = Some(tempo);
                }
                if child.attribute("dacapo") == Some("yes") {
                    sound_dacapo = true;
                }
                if child.attribute("dalsegno").is_some() {
                    sound_dalsegno = true;
                }
                if child.attribute("fine") == Some("yes") {
                    sound_fine = true;
                }
                if child.attribute("tocoda").is_some() {
                    sound_tocoda = true;
                }
            }
            _ => {}
        }
    }

    // Return a Direction if it has any useful content
    let has_content = sound_tempo.is_some()
        || metronome.is_some()
        || words.is_some()
        || segno
        || coda
        || rehearsal.is_some()
        || sound_dacapo
        || sound_dalsegno
        || sound_fine
        || sound_tocoda
        || octave_shift_type.is_some();

    if has_content {
        Some(Direction {
            placement,
            sound_tempo,
            metronome,
            words,
            segno,
            coda,
            rehearsal,
            sound_dacapo,
            sound_dalsegno,
            sound_fine,
            sound_tocoda,
            words_font_style,
            octave_shift_type,
            octave_shift_size,
        })
    } else {
        None
    }
}

fn parse_metronome(node: &Node) -> MetronomeMark {
    let mut beat_unit = "quarter".to_string();
    let mut per_minute = 120;
    let mut dotted = false;

    for child in node.children().filter(|n| n.is_element()) {
        match child.tag_name().name() {
            "beat-unit" => {
                beat_unit = child.text().unwrap_or("quarter").trim().to_string();
            }
            "beat-unit-dot" => {
                dotted = true;
            }
            "per-minute" => {
                per_minute = child
                    .text()
                    .and_then(|t| t.trim().parse::<f64>().ok())
                    .map(|v| v as i32)
                    .unwrap_or(120);
            }
            _ => {}
        }
    }

    MetronomeMark {
        beat_unit,
        per_minute,
        dotted,
    }
}

// ─── Helpers ─────────────────────────────────────────────────────────

fn parse_i32(node: &Node) -> Option<i32> {
    node.text()?.trim().parse().ok()
}

fn parse_f64(node: &Node) -> Option<f64> {
    node.text()?.trim().parse().ok()
}
