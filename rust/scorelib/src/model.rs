//! Data model for representing a parsed MusicXML score.
//!
//! These structures capture the essential musical information needed
//! for rendering sheet music and audio playback.

use serde::{Deserialize, Serialize};

/// Font/style attributes parsed from a MusicXML `<credit-words>` element.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct TextStyle {
    /// Font family (e.g., "Times New Roman", "Arial")
    pub font_family: Option<String>,
    /// Font size in points (e.g., 22.0)
    pub font_size: Option<f64>,
    /// Font weight (e.g., "bold", "normal")
    pub font_weight: Option<String>,
    /// Font style (e.g., "italic", "normal")
    pub font_style: Option<String>,
}

/// A complete musical score parsed from MusicXML.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Score {
    /// Title of the piece
    pub title: Option<String>,
    /// Style for the title text (from `<credit-words>` attributes)
    pub title_style: Option<TextStyle>,
    /// Subtitle
    pub subtitle: Option<String>,
    /// Style for the subtitle text
    pub subtitle_style: Option<TextStyle>,
    /// Composer name
    pub composer: Option<String>,
    /// Style for the composer text
    pub composer_style: Option<TextStyle>,
    /// Arranger name
    pub arranger: Option<String>,
    /// MusicXML version (e.g., "3.1", "4.0")
    pub version: Option<String>,
    /// Software that created the file
    pub software: Option<String>,
    /// Page layout defaults
    pub defaults: Option<Defaults>,
    /// Musical parts (instruments)
    pub parts: Vec<Part>,
}

/// Page layout and font defaults.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Defaults {
    /// Scaling: millimeters per tenths
    pub millimeters: Option<f64>,
    pub tenths: Option<f64>,
    /// Page dimensions in tenths
    pub page_height: Option<f64>,
    pub page_width: Option<f64>,
    /// Page margins in tenths
    pub left_margin: Option<f64>,
    pub right_margin: Option<f64>,
    pub top_margin: Option<f64>,
    pub bottom_margin: Option<f64>,
}

/// A musical part (one instrument or voice).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Part {
    /// Part identifier (e.g., "P1")
    pub id: String,
    /// Part name (e.g., "Classical Guitar")
    pub name: String,
    /// Abbreviated name (e.g., "Guit.")
    pub abbreviation: Option<String>,
    /// MIDI program number
    pub midi_program: Option<i32>,
    /// MIDI channel
    pub midi_channel: Option<i32>,
    /// Ordered list of measures
    pub measures: Vec<Measure>,
}

/// A single measure (bar) of music.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Measure {
    /// Measure number
    pub number: i32,
    /// Whether this is an implicit measure (e.g., pickup/anacrusis)
    pub implicit: bool,
    /// Width in tenths (for layout)
    pub width: Option<f64>,
    /// Attributes (key, time, clef) — only present when they change
    pub attributes: Option<Attributes>,
    /// Notes and rests in this measure
    pub notes: Vec<Note>,
    /// Chord symbols
    pub harmonies: Vec<Harmony>,
    /// Barlines (repeat signs, double bars, etc.)
    pub barlines: Vec<Barline>,
    /// Direction elements (tempo, dynamics, text expressions)
    pub directions: Vec<Direction>,
    /// Whether this measure starts a new system (line break)
    pub new_system: bool,
    /// Whether this measure starts a new page
    pub new_page: bool,
}

/// Musical attributes that may change at the start of a measure.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Attributes {
    /// Divisions per quarter note (determines duration resolution)
    pub divisions: Option<i32>,
    /// Key signature
    pub key: Option<Key>,
    /// Time signature
    pub time: Option<TimeSignature>,
    /// Clef(s) — one per staff.  For single-staff parts this holds one
    /// element; for grand-staff (piano) parts it holds two or more,
    /// each tagged with a staff `number`.
    pub clefs: Vec<Clef>,
    /// Transposition
    pub transpose: Option<Transpose>,
    /// Number of staves in this part (e.g. 2 for piano grand staff)
    pub staves: Option<i32>,
}

/// Key signature.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Key {
    /// Number of sharps (positive) or flats (negative)
    pub fifths: i32,
    /// Mode (e.g., "major", "minor")
    pub mode: Option<String>,
}

/// Time signature.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TimeSignature {
    /// Numerator (e.g., 3 in 3/4)
    pub beats: i32,
    /// Denominator (e.g., 4 in 3/4)
    pub beat_type: i32,
}

/// Clef definition.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Clef {
    /// Staff number this clef belongs to (1-based; defaults to 1)
    pub number: i32,
    /// Clef sign: "G" (treble), "F" (bass), "C" (alto/tenor)
    pub sign: String,
    /// Staff line the clef sits on
    pub line: i32,
    /// Octave transposition (e.g., -1 for guitar's octave-lower treble clef)
    pub octave_change: Option<i32>,
}

/// Transposition information.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Transpose {
    pub diatonic: i32,
    pub chromatic: i32,
    pub octave_change: Option<i32>,
}

/// A single note or rest.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Note {
    /// Pitch (None if this is a rest)
    pub pitch: Option<Pitch>,
    /// Duration in divisions
    pub duration: i32,
    /// Voice number (for multi-voice writing)
    pub voice: Option<i32>,
    /// Note type: "whole", "half", "quarter", "eighth", "16th", "32nd"
    pub note_type: Option<String>,
    /// Stem direction: "up" or "down"
    pub stem: Option<String>,
    /// Beam information
    pub beams: Vec<Beam>,
    /// Whether this is a rest
    pub rest: bool,
    /// Whether this rest fills the entire measure (MusicXML: `<rest measure="yes"/>`)
    pub measure_rest: bool,
    /// Whether this note is part of a chord with the previous note
    pub chord: bool,
    /// Whether the note has a dot
    pub dot: bool,
    /// Accidental: "sharp", "flat", "natural", "double-sharp", "flat-flat"
    pub accidental: Option<String>,
    /// Whether this note starts a tie (held into the next note)
    pub tie_start: bool,
    /// Whether this note stops a tie (continuation from a previous note)
    pub tie_stop: bool,
    /// Staff number (1-based; for multi-staff parts like piano)
    pub staff: Option<i32>,
    /// Default X position in tenths (for layout)
    pub default_x: Option<f64>,
    /// Default Y position in tenths (for layout)
    pub default_y: Option<f64>,
    /// Lyrics attached to this note
    pub lyrics: Vec<Lyric>,
    /// Whether this is a grace note
    pub grace: bool,
    /// Whether this grace note has a slash (acciaccatura vs appoggiatura)
    pub grace_slash: bool,
    /// Slur events on this note (start/stop)
    pub slurs: Vec<SlurEvent>,
}

/// A slur start or stop event on a note.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SlurEvent {
    /// "start" or "stop"
    pub slur_type: String,
    /// Slur ID for matching pairs (1, 2, 3…)
    pub number: i32,
    /// Placement hint from MusicXML: "above" or "below"
    pub placement: Option<String>,
}

/// A lyric syllable attached to a note.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Lyric {
    /// Lyric line number (e.g. 1 for verse 1, 2 for verse 2)
    pub number: i32,
    /// The text of this syllable
    pub text: String,
    /// Syllabic type: "single", "begin", "middle", "end"
    pub syllabic: Option<String>,
}

/// Pitch of a note.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Pitch {
    /// Note name: A, B, C, D, E, F, G
    pub step: String,
    /// Octave number (middle C = C4)
    pub octave: i32,
    /// Chromatic alteration: -1.0 = flat, 1.0 = sharp, 0.0 = natural
    pub alter: Option<f64>,
}

/// Beam grouping information.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Beam {
    /// Beam level (1 = eighth-note beam, 2 = sixteenth-note beam, etc.)
    pub number: i32,
    /// Beam type: "begin", "continue", "end"
    pub beam_type: String,
}

/// A chord symbol (harmony).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Harmony {
    /// Root note
    pub root: HarmonyRoot,
    /// Chord quality: "major", "minor", "dominant", "diminished", etc.
    pub kind: String,
    /// Bass note (for slash chords)
    pub bass: Option<HarmonyRoot>,
}

/// Root or bass note of a harmony.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HarmonyRoot {
    /// Note name: A–G
    pub step: String,
    /// Alteration: -1 = flat, 1 = sharp
    pub alter: Option<f64>,
}

/// A barline (may include repeat signs).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Barline {
    /// Location: "left", "right", "middle"
    pub location: String,
    /// Visual style: "regular", "light-light", "light-heavy", "heavy-light", etc.
    pub bar_style: Option<String>,
    /// Repeat sign
    pub repeat: Option<Repeat>,
    /// Volta bracket (1st/2nd ending)
    pub ending: Option<Ending>,
}

/// A repeat sign on a barline.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Repeat {
    /// "forward" or "backward"
    pub direction: String,
}

/// A volta bracket (1st/2nd ending).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Ending {
    /// Ending number(s), e.g., "1", "2", "1, 2"
    pub number: String,
    /// "start", "stop", or "discontinue"
    pub ending_type: String,
    /// Display text
    pub text: Option<String>,
}

/// A direction element (tempo, dynamics, text expressions, etc.).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Direction {
    /// Placement: "above" or "below" the staff
    pub placement: Option<String>,
    /// Tempo from <sound tempo="..."> (in BPM)
    pub sound_tempo: Option<f64>,
    /// Metronome marking from <direction-type>/<metronome>
    pub metronome: Option<MetronomeMark>,
    /// Text words from <direction-type>/<words> (e.g., "Allegro", "rit.")
    pub words: Option<String>,
    /// Whether this direction contains a segno sign
    pub segno: bool,
    /// Whether this direction contains a coda sign
    pub coda: bool,
    /// Rehearsal mark text (e.g. "A", "B", "C")
    pub rehearsal: Option<String>,
    /// Navigation jump from <sound>: "dacapo", "dalsegno", "fine", "tocoda"
    pub sound_dacapo: bool,
    pub sound_dalsegno: bool,
    pub sound_fine: bool,
    pub sound_tocoda: bool,
    /// Font style for <words>: "italic", "bold", "bold italic"
    pub words_font_style: Option<String>,
    /// Octave-shift type: "down" (8va — display lower), "up" (8vb — display higher), "stop"
    #[serde(default)]
    pub octave_shift_type: Option<String>,
    /// Octave-shift size: 8 (1 octave), 15 (2 octaves), 22 (3 octaves)
    #[serde(default)]
    pub octave_shift_size: i32,
}

/// A metronome marking (e.g., quarter = 120).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MetronomeMark {
    /// Beat unit: "whole", "half", "quarter", "eighth", etc.
    pub beat_unit: String,
    /// Beats per minute
    pub per_minute: i32,
    /// Whether the beat unit is dotted
    pub dotted: bool,
}

impl Score {
    /// Create a new empty score.
    pub fn new() -> Self {
        Self {
            title: None,
            title_style: None,
            subtitle: None,
            subtitle_style: None,
            composer: None,
            composer_style: None,
            arranger: None,
            version: None,
            software: None,
            defaults: None,
            parts: Vec::new(),
        }
    }

    /// Get the total number of measures across all parts.
    pub fn measure_count(&self) -> usize {
        self.parts.first().map_or(0, |p| p.measures.len())
    }

    /// Get all unique time signatures used in the score.
    pub fn time_signatures(&self) -> Vec<&TimeSignature> {
        let mut sigs = Vec::new();
        for part in &self.parts {
            for measure in &part.measures {
                if let Some(ref attrs) = measure.attributes {
                    if let Some(ref ts) = attrs.time {
                        if !sigs.iter().any(|s: &&TimeSignature| {
                            s.beats == ts.beats && s.beat_type == ts.beat_type
                        }) {
                            sigs.push(ts);
                        }
                    }
                }
            }
        }
        sigs
    }
}

impl Default for Score {
    fn default() -> Self {
        Self::new()
    }
}

impl Pitch {
    /// Convert pitch to MIDI note number.
    /// Middle C (C4) = 60.
    pub fn to_midi(&self) -> i32 {
        let step_semitone = match self.step.as_str() {
            "C" => 0,
            "D" => 2,
            "E" => 4,
            "F" => 5,
            "G" => 7,
            "A" => 9,
            "B" => 11,
            _ => 0,
        };
        let alter = self.alter.unwrap_or(0.0) as i32;
        (self.octave + 1) * 12 + step_semitone + alter
    }
}
