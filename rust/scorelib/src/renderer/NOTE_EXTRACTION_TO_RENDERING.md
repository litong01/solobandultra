# From note extraction to rendering

This doc describes what happens **after** note extraction and **before** rendering, and why you might see only one note per measure even when extraction returns several.

## Two different “note” lists

1. **Note timeline** (for feedback/overlay)  
   - Built in `note_timeline::generate_note_timeline()`.  
   - Uses **voice 1 only** (no staff filter); includes every voice-1 note (including chord notes).  
   - Produces `(measure_idx, note_idx)` where `note_idx` = index among melody notes in that measure (0, 1, 2, …).

2. **Top-layer extraction** (for tests / selection)  
   - Built in `top_layer::extract_top_layer_from_part(part, staff_filter)`.  
   - One note per (staff, beat): top voice on that staff + highest pitch at each chord.  
   - Used by tests; can be used for “which notes are selectable” in the app.

## Rendering pipeline (SVG)

1. **Layout** (`layout.rs`)  
   - For each measure we build `all_beat_times`: for each part, `compute_note_beat_times(part.measures[mi].notes, divisions)` — **all notes** (all voices, all staves).  
   - `compute_beat_x_map(all_beat_times, …)` builds **unique** beat times (sorted), then assigns an x position to each.  
   - Result: `beat_x_map` = `[(beat_time, svg_x), …]` — **one entry per unique beat** in the measure (across all notes).

2. **Rendering notes** (`notes.rs`, `jianpu.rs`)  
   - `note_x_positions_from_beat_map(measure.notes, divisions, beat_x_map)` assigns each note an x from its beat time.  
   - So **every note** in `measure.notes` gets a position and is drawn (subject to staff filter).  
   - If `beat_x_map` had only one entry, every note would get the **same** x (stacked), which can look like “one note”.

3. **Playback map** (`playback.rs`)  
   - Takes the same `beat_x_map` and turns it into `note_positions: Vec<(f64, f64)>` = `[(time_fraction, svg_x), …]` (+ anchor at 1.0).  
   - So **`note_positions` is indexed by “unique beat index”**, not by “melody note index”.

## Where “only one note” can come from

- **Playback cursor / overlay**  
  The app uses `note_positions[noteIdx]` with `noteIdx` from the **note timeline** (melody note index).  
  That only matches if the **first K unique beats** in the measure are exactly the **K melody note beats** in order. If another voice has a note at an earlier beat, indices shift and you can get wrong or single positions.

- **Only one position in `beat_x_map`**  
  If layout ever produced a single `(beat, x)` per measure, then:  
  - All notes would get the same x (stacked).  
  - `note_positions` would have length 1 (or 2 with the 1.0 anchor), so cursor/overlay would only show one position.  
  That can happen if `all_beat_times` is built from a **filtered** note set that collapses to one beat (e.g. only one slot for jianpu in some code path).

## Recommended alignment fix

To make overlay/selection reliable:

1. **Build playback-map note positions from the same notion of “melody” as the note timeline.**  
   For each measure, use the same extraction as in the test (e.g. top-layer with `staff_filter = selected_staff`), then:
   - For each extracted note, compute its beat time (already available in top_layer).
   - Resolve beat time → x using the same `beat_x_map` (or the same layout logic).
   - Store **one** `(fraction, x)` per **melody/top-layer note** in order.  
   Then `note_positions[i]` = position of the i-th melody note, and `noteIdx` from the note timeline always matches.

2. **Alternatively**, keep building `note_positions` from unique beats, but add a separate **melody-to-position** map (e.g. for each `(measure_idx, note_idx)` return the corresponding `(fraction, x)`). The overlay would then use that map instead of indexing `note_positions` by `noteIdx`.

## Summary

| Stage              | Data source              | Index meaning                          |
|--------------------|--------------------------|----------------------------------------|
| Note timeline      | Voice 1 (no staff filter)| `note_idx` = melody note index        |
| Top-layer (tests)  | Per-staff top voice      | One note per (staff, beat)             |
| Layout beat_x_map  | All notes, all voices   | One entry per **unique** beat         |
| Playback note_positions | Same as beat_x_map  | Unique-beat index (not melody index)   |
| Overlay            | note_positions[noteIdx]  | Assumes noteIdx = position index       |

If you see only one note in the measure, check: (1) does `beat_x_map` for that measure have more than one entry? (2) does the app use `noteIdx` as index into `note_positions` and is that intended to be melody index? Aligning playback-map positions with melody/top-layer extraction (same as the test) will fix the mismatch.
