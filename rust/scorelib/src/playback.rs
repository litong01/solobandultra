//! Playback map: maps visual measure positions in the SVG to timing
//! information from the unrolled timemap. This is the bridge between
//! MIDI playback time and the cursor position in the rendered SVG.
//!
//! The cursor uses **measure bounding boxes** and **linear interpolation**
//! (ported from the mysoloband TypeScript implementation):
//!   `cursor_x = measure.x + (offset / duration) * measure.width`

use serde::Serialize;

use crate::model::Score;
use crate::renderer::compute_measure_positions;
use crate::timemap::{self, TimemapEntry};
use crate::unroller;

/// Complete playback map combining visual positions with timing data.
#[derive(Debug, Clone, Serialize)]
pub struct PlaybackMap {
    /// Visual position of each original measure in the SVG.
    pub measures: Vec<MeasurePosition>,
    /// Visual position of each system (line of music) in the SVG.
    pub systems: Vec<SystemPosition>,
    /// Timing data for each measure in the unrolled (play-order) sequence.
    /// Each entry maps to an original measure via `original_index`.
    pub timemap: Vec<TimemapEntryJson>,
}

/// Visual position of a measure in the SVG coordinate space.
#[derive(Debug, Clone, Serialize)]
pub struct MeasurePosition {
    /// Index into Part.measures (original measure index)
    pub measure_idx: usize,
    /// X coordinate of the measure's left edge in SVG units
    pub x: f64,
    /// Width of the measure in SVG units
    pub width: f64,
    /// Which system (line) this measure belongs to (0-based)
    pub system_idx: usize,
}

/// Visual position and dimensions of a system (line of music).
#[derive(Debug, Clone, Serialize)]
pub struct SystemPosition {
    /// Y coordinate of the system's top staff line
    pub y: f64,
    /// Total height of the system (staves + lyrics + spacing)
    pub height: f64,
}

/// Serializable version of TimemapEntry for JSON output.
#[derive(Debug, Clone, Serialize)]
pub struct TimemapEntryJson {
    /// Index in the unrolled sequence
    pub index: usize,
    /// Index into the original Part.measures
    pub original_index: usize,
    /// Start time in milliseconds
    pub timestamp_ms: f64,
    /// Duration in milliseconds
    pub duration_ms: f64,
    /// Tempo at this measure (BPM)
    pub tempo_bpm: f64,
}

impl From<&TimemapEntry> for TimemapEntryJson {
    fn from(e: &TimemapEntry) -> Self {
        Self {
            index: e.index,
            original_index: e.original_index,
            timestamp_ms: e.timestamp_ms,
            duration_ms: e.duration_ms,
            tempo_bpm: e.tempo_bpm,
        }
    }
}

/// Generate a playback map for a score at the given page width.
///
/// This computes the same layout as `render_score_to_svg` but only
/// extracts the measure and system positions — no actual SVG is produced.
/// Combined with the unrolled timemap, this gives the WebView everything
/// it needs to position and animate the playback cursor.
pub fn generate_playback_map(score: &Score, page_width: Option<f64>) -> PlaybackMap {
    // Get measure and system positions from the renderer's layout
    let (measure_positions, system_positions) = compute_measure_positions(score, page_width);

    // Unroll and generate timemap
    let part_idx = 0;
    let unrolled = unroller::unroll(score, part_idx);
    let tmap = timemap::generate_timemap(score, part_idx, &unrolled);

    let measures = measure_positions
        .into_iter()
        .map(|(measure_idx, x, width, system_idx)| MeasurePosition {
            measure_idx,
            x,
            width,
            system_idx,
        })
        .collect();

    let systems = system_positions
        .into_iter()
        .map(|(y, height)| SystemPosition { y, height })
        .collect();

    let timemap_json = tmap.iter().map(TimemapEntryJson::from).collect();

    PlaybackMap {
        measures,
        systems,
        timemap: timemap_json,
    }
}

/// Serialize a PlaybackMap to JSON.
pub fn playback_map_to_json(map: &PlaybackMap) -> String {
    serde_json::to_string(map).unwrap_or_else(|_| "{}".to_string())
}
