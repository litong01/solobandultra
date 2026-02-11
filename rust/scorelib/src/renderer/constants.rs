//! Shared constants for the score renderer (all in SVG user units).

// ── Page & margins ──────────────────────────────────────────────────
pub(super) const DEFAULT_PAGE_WIDTH: f64 = 820.0;
pub(super) const PAGE_MARGIN_LEFT: f64 = 50.0;
pub(super) const PAGE_MARGIN_RIGHT: f64 = 30.0;
pub(super) const PAGE_MARGIN_TOP: f64 = 30.0;

// ── Staff dimensions ────────────────────────────────────────────────
pub(super) const STAFF_LINE_SPACING: f64 = 10.0; // distance between staff lines
pub(super) const STAFF_HEIGHT: f64 = 40.0; // 5 lines, 4 spaces
pub(super) const SYSTEM_SPACING: f64 = 90.0; // vertical space between systems
pub(super) const GRAND_STAFF_GAP: f64 = 60.0; // vertical gap between staves in a grand staff
pub(super) const PART_GAP: f64 = 80.0; // vertical gap between different parts/instruments
pub(super) const BRACE_WIDTH: f64 = 10.0; // width of the brace/bracket

// ── Header ──────────────────────────────────────────────────────────
pub(super) const HEADER_HEIGHT: f64 = 70.0; // space for title + composer
pub(super) const FIRST_SYSTEM_TOP: f64 = PAGE_MARGIN_TOP + HEADER_HEIGHT;

// ── Prefix widths ───────────────────────────────────────────────────
pub(super) const CLEF_SPACE: f64 = 32.0; // horizontal space for clef at system start
pub(super) const KEY_SIG_SHARP_SPACE: f64 = 10.0;
pub(super) const KEY_SIG_FLAT_SPACE: f64 = 8.0;
pub(super) const KEY_SIG_NATURAL_SPACE: f64 = 8.0;
pub(super) const TIME_SIG_SPACE: f64 = 24.0;

// ── Note dimensions ─────────────────────────────────────────────────
pub(super) const NOTEHEAD_RX: f64 = 5.5; // notehead ellipse x-radius
pub(super) const NOTEHEAD_RY: f64 = 4.0; // notehead ellipse y-radius
pub(super) const STEM_LENGTH: f64 = 30.0;
pub(super) const STEM_WIDTH: f64 = 1.2;
pub(super) const BEAM_THICKNESS: f64 = 4.0;
pub(super) const BARLINE_WIDTH: f64 = 1.0;
pub(super) const STAFF_LINE_WIDTH: f64 = 0.8;
pub(super) const LEDGER_LINE_WIDTH: f64 = 0.8;
pub(super) const LEDGER_LINE_EXTEND: f64 = 5.0;

// ── Measure packing ─────────────────────────────────────────────────
pub(super) const MIN_MEASURE_WIDTH: f64 = 38.0;
pub(super) const PER_BEAT_MIN_WIDTH: f64 = 55.0;
pub(super) const CHORD_SYMBOL_OFFSET_Y: f64 = -18.0; // above staff

// ── Colors ──────────────────────────────────────────────────────────
pub(super) const NOTE_COLOR: &str = "#1a1a1a";
pub(super) const STAFF_COLOR: &str = "#555555";
pub(super) const BARLINE_COLOR: &str = "#333333";
pub(super) const CHORD_COLOR: &str = "#4a4a9a";
pub(super) const HEADER_COLOR: &str = "#1a1a1a";
pub(super) const REST_COLOR: &str = "#1a1a1a";
