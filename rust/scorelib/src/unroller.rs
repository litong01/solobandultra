//! Unroll a score by expanding repeats and navigation jumps into a linear
//! measure sequence.  This is the foundation for MIDI generation — we need
//! a flat, play-order list of measures before computing timestamps.
//!
//! Handles:
//! - Forward / backward repeat barlines
//! - Volta brackets (1st / 2nd / Nth endings)
//! - D.S. (dal segno) — jump to segno
//! - D.C. (da capo) — jump to beginning
//! - Fine — stop on D.S./D.C. pass (detected from `<sound fine>` or "Fine" text)
//! - To Coda / Coda — jump to coda section
//! - Senza ripetizione: repeats are NOT taken again after a D.S./D.C. jump

use crate::model::Score;

/// One entry in the unrolled (play-order) sequence.
#[derive(Debug, Clone)]
pub struct UnrolledMeasure {
    /// Index into `Part.measures` for the original measure data.
    pub original_index: usize,
}

/// Unroll a single part's measures into play order.
///
/// The algorithm walks through the measures linearly, following repeat
/// barlines (with volta handling), and navigation jumps (D.S., D.C.,
/// Coda, Fine).  Each part should be unrolled independently since they
/// share the same structure in standard MusicXML.
pub fn unroll(score: &Score, part_idx: usize) -> Vec<UnrolledMeasure> {
    let part = match score.parts.get(part_idx) {
        Some(p) => p,
        None => return Vec::new(),
    };
    let measures = &part.measures;
    if measures.is_empty() {
        return Vec::new();
    }

    // ── Pre-scan: locate segno, coda markers ────────────────────────
    let mut segno_index: Option<usize> = None;
    let mut coda_index: Option<usize> = None;

    for (i, m) in measures.iter().enumerate() {
        for dir in &m.directions {
            if dir.segno {
                segno_index = Some(i);
            }
            if dir.coda {
                coda_index = Some(i);
            }
        }
    }

    // ── Pre-scan: build volta (ending) map ──────────────────────────
    // Maps measure index → ending number string (e.g. "1", "2").
    let mut volta_map: std::collections::HashMap<usize, Vec<i32>> = std::collections::HashMap::new();
    let mut current_ending: Option<Vec<i32>> = None;
    for (i, m) in measures.iter().enumerate() {
        for bl in &m.barlines {
            if let Some(ref ending) = bl.ending {
                match ending.ending_type.as_str() {
                    "start" => {
                        let nums = parse_ending_numbers(&ending.number);
                        current_ending = Some(nums.clone());
                        volta_map.insert(i, nums);
                    }
                    "stop" | "discontinue" => {
                        if let Some(ref nums) = current_ending {
                            volta_map.entry(i).or_insert_with(|| nums.clone());
                        }
                        current_ending = None;
                    }
                    _ => {}
                }
            }
        }
        if let Some(ref nums) = current_ending {
            volta_map.entry(i).or_insert_with(|| nums.clone());
        }
    }

    // ── Pre-scan: compute max passes per repeat section ────────────
    // For each forward-repeat position, find the highest volta ending
    // number in its section.  This tells us how many passes to take.
    let mut section_max_passes: std::collections::HashMap<usize, i32> = std::collections::HashMap::new();
    {
        let mut current_forward: Option<usize> = None;
        for (i, m) in measures.iter().enumerate() {
            for bl in &m.barlines {
                if bl.location == "left" {
                    if let Some(ref rep) = bl.repeat {
                        if rep.direction == "forward" {
                            current_forward = Some(i);
                        }
                    }
                }
            }
            if let Some(nums) = volta_map.get(&i) {
                if let Some(fwd) = current_forward {
                    let entry = section_max_passes.entry(fwd).or_insert(2);
                    for &n in nums {
                        if n > *entry {
                            *entry = n;
                        }
                    }
                }
            }
        }
    }

    // ── Walk: expand into play order ────────────────────────────────
    let mut result: Vec<UnrolledMeasure> = Vec::new();
    let mut pos: usize = 0;
    let mut repeat_start: usize = 0;
    let mut repeat_pass: i32 = 1; // 1-based pass counter (1st, 2nd, 3rd, …)
    let mut jump_taken = false;
    let max_iterations = measures.len() * 8; // safety limit
    let mut iterations = 0;

    while pos < measures.len() {
        iterations += 1;
        if iterations > max_iterations {
            break;
        }

        let m = &measures[pos];

        // Check for forward repeat barline (left barline).
        // Only update repeat_start on the very first encounter (pass 1);
        // on subsequent passes we're jumping back here, so don't reset.
        for bl in &m.barlines {
            if bl.location == "left" {
                if let Some(ref rep) = bl.repeat {
                    if rep.direction == "forward" && repeat_pass == 1 {
                        repeat_start = pos;
                    }
                }
            }
        }

        // Check volta: if this measure has an ending number and we're
        // on a repeat pass that doesn't match, skip it.
        if let Some(nums) = volta_map.get(&pos) {
            if !nums.contains(&repeat_pass) {
                pos += 1;
                continue;
            }
        }

        // Check for Fine — stop if we've already taken a D.S./D.C. jump
        // and this measure has a Fine marker.
        if jump_taken && measure_has_fine(m) {
            // Emit this measure, then stop
            result.push(UnrolledMeasure { original_index: pos });
            break;
        }

        // Check for "To Coda" (when we've already taken a D.S./D.C. jump)
        if jump_taken {
            for dir in &m.directions {
                if dir.sound_tocoda {
                    if let Some(coda_idx) = coda_index {
                        pos = coda_idx;
                        jump_taken = false; // reset so we don't loop
                        continue;
                    }
                }
            }
        }

        // Emit this measure
        result.push(UnrolledMeasure { original_index: pos });

        // Check for backward repeat barline (right barline).
        // SENZA RIPETIZIONE: after a D.S./D.C. jump, repeats are NOT taken.
        let mut took_repeat = false;
        if !jump_taken {
            for bl in &m.barlines {
                if bl.location == "right" {
                    if let Some(ref rep) = bl.repeat {
                        if rep.direction == "backward" {
                            // Determine how many total passes this section needs
                            // from the pre-computed map; default to 2 (simple repeat).
                            let max_pass = section_max_passes
                                .get(&repeat_start)
                                .copied()
                                .unwrap_or(2);
                            if repeat_pass < max_pass {
                                repeat_pass += 1;
                                pos = repeat_start;
                                took_repeat = true;
                                break;
                            }
                            // Last pass done — fall through to continue forward.
                        }
                    }
                }
            }
        }
        if took_repeat {
            continue;
        }

        // Check for D.S. / D.C. jumps (in directions)
        if !jump_taken {
            let mut jumped = false;
            for dir in &m.directions {
                if dir.sound_dalsegno {
                    if let Some(segno_idx) = segno_index {
                        pos = segno_idx;
                        jump_taken = true;
                        repeat_pass = 1;
                        jumped = true;
                        break;
                    }
                }
                if dir.sound_dacapo {
                    pos = 0;
                    jump_taken = true;
                    repeat_pass = 1;
                    jumped = true;
                    break;
                }
            }
            if jumped {
                continue;
            }
        }

        pos += 1;
        // Reset repeat pass when we've finished all passes and move past
        // the last volta bracket in a repeat section.
        if repeat_pass > 1 {
            let prev_had_backward = measures.get(pos.wrapping_sub(1)).map_or(false, |pm| {
                pm.barlines.iter().any(|bl| {
                    bl.location == "right"
                        && bl.repeat.as_ref().map_or(false, |r| r.direction == "backward")
                })
            });
            if prev_had_backward && !volta_map.contains_key(&pos) {
                repeat_pass = 1;
            }
        }
    }

    result
}

/// Check if a measure contains a Fine marker, either via `<sound fine="yes">`
/// or via "Fine" in direction words text (excluding "D.S. al Fine" / "D.C. al Fine").
fn measure_has_fine(m: &crate::model::Measure) -> bool {
    m.directions.iter().any(|dir| {
        if dir.sound_fine {
            return true;
        }
        // Fallback: "Fine" in words text (case-insensitive), but skip
        // "D.S. al Fine" / "D.C. al Fine" which are jump instructions.
        if let Some(ref w) = dir.words {
            let lower = w.to_lowercase();
            if lower.contains("fine") && !lower.contains("d.s.") && !lower.contains("d.c.") {
                return true;
            }
        }
        false
    })
}

/// Parse ending number string like "1", "2", "1, 2" into a vec of ints.
fn parse_ending_numbers(s: &str) -> Vec<i32> {
    s.split(|c: char| c == ',' || c == ' ')
        .filter_map(|part| part.trim().parse::<i32>().ok())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parse_file;

    #[test]
    fn unroll_asa_branca_has_repeats() {
        let score = parse_file("../../sheetmusic/asa-branca.musicxml").unwrap();
        let unrolled = unroll(&score, 0);
        let raw_count = score.parts[0].measures.len();
        assert!(
            unrolled.len() > raw_count,
            "Unrolled length {} should be > raw measure count {}",
            unrolled.len(), raw_count
        );
        println!("asa-branca: {} raw measures → {} unrolled measures", raw_count, unrolled.len());
    }

    #[test]
    fn unroll_tongnian_has_multiple_endings() {
        let score = parse_file("../../sheetmusic/童年.mxl").unwrap();
        let unrolled = unroll(&score, 0);
        let raw_count = score.parts[0].measures.len();

        // 童年 has:
        //   - Intro (2 measures, repeated once = 4)
        //   - Body gap (4 measures)
        //   - Main section (16 body + 1 ending) × 6 volta endings = 102
        //   - Bridge (1 measure)
        //   - Chorus (3 body + 1 ending) × 2 volta endings = 8
        //   - Coda (6 measures)
        // Total: 4 + 4 + 102 + 1 + 8 + 6 = 125
        assert!(
            unrolled.len() > raw_count,
            "Unrolled length {} should be > raw measure count {} (multiple endings)",
            unrolled.len(), raw_count
        );
        println!("童年: {} raw measures → {} unrolled measures", raw_count, unrolled.len());
    }

    #[test]
    fn unroll_blue_bag_folly_has_ds_al_fine() {
        let score = parse_file("../../sheetmusic/blue-bag-folly.musicxml").unwrap();
        let unrolled = unroll(&score, 0);
        let raw_count = score.parts[0].measures.len();
        assert!(
            unrolled.len() > raw_count,
            "Unrolled length {} should be > raw measure count {} (D.S. al Fine)",
            unrolled.len(), raw_count
        );
        println!("blue-bag-folly: {} raw measures → {} unrolled measures", raw_count, unrolled.len());
    }
}
