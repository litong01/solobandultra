# How note position and lyrics position are computed (jianpu)

## 1. Where does the "note position" come from?

- **Source:** `note_x_positions_from_beat_map(notes, divisions, beat_x_map)` in `beat_map.rs`.
- **Meaning:** For each note we get one x: the x from the beat map for that note’s **beat time** (grace notes are shifted left of the principal). The code uses this as the note’s **center x** (`nx`) for layout and drawing.

---

## 2. Simplified: one width for all note heads (0–7)

We treat every jianpu note head as having the **same** width, regardless of duration. The duration suffix (e.g. `"."`, `" -"`) is **not** included in that width — it extends to the right of the head.

- **Note head width:** `jianpu_note_head_width(font_size)` returns the font size (one fixed width for all note heads).
- **Drawing:** We call `jianpu_note_text_centered(nx, y, content, w, 0.0, ...)` with `w = jianpu_note_head_width(JIANPU_FONT_SIZE)` for every note. So `x_start = nx - w/2`; the **note head** is centered at `nx`. The full string (e.g. `"6."`) may extend right; we do not use that for centering.
- **Lyrics:** We use `center_x = note_positions[i]` (same `nx`) with `text-anchor="middle"`. So the lyric is centered at `nx`, same as the note head.

Result: note head and lyric share the same center `nx`. No per-note full-width or x_c logic; one constant head width for all 0–7.
