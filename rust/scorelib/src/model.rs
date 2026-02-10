//! Data model for representing a parsed MusicXML score.
//!
//! These structures capture the essential musical information needed
//! for rendering sheet music and audio playback.

use serde::{Deserialize, Serialize};

/// A complete musical score parsed from MusicXML.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Score {
    /// Title of the piece
    pub title: Option<String>,
    /// Subtitle
    pub subtitle: Option<String>,
    /// Composer name
    pub composer: Option<String>,
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
    /// Whether this note is part of a chord with the previous note
    pub chord: bool,
    /// Whether the note has a dot
    pub dot: bool,
    /// Accidental: "sharp", "flat", "natural", "double-sharp", "flat-flat"
    pub accidental: Option<String>,
    /// Tie: "start", "stop"
    pub tie: Option<String>,
    /// Staff number (1-based; for multi-staff parts like piano)
    pub staff: Option<i32>,
    /// Default X position in tenths (for layout)
    pub default_x: Option<f64>,
    /// Default Y position in tenths (for layout)
    pub default_y: Option<f64>,
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

impl Score {
    /// Create a new empty score.
    pub fn new() -> Self {
        Self {
            title: None,
            subtitle: None,
            composer: None,
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
