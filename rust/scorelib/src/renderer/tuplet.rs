//! Tuplet group detection — finds runs of notes that form tuplets (e.g. triplets)
//! from time-modification and notations/tuplet in the MusicXML.

use crate::model::Measure;

/// Find tuplet groups: each group is a list of note indices (principal + chord)
/// that form one tuplet (e.g. three eighth notes in the space of two).
/// Returns `(group_indices, actual_notes)` per group.
pub(super) fn find_tuplet_groups(
    measure: &Measure,
    staff_filter: Option<i32>,
) -> Vec<(Vec<usize>, i32)> {
    let mut groups: Vec<(Vec<usize>, i32)> = Vec::new();
    let mut i = 0;
    while i < measure.notes.len() {
        let note = &measure.notes[i];
        if note.chord || note.rest || note.grace {
            i += 1;
            continue;
        }
        if let Some(sf) = staff_filter {
            if note.staff.unwrap_or(1) != sf {
                i += 1;
                continue;
            }
        }

        let actual_notes = note
            .time_modification
            .as_ref()
            .map(|tm| tm.actual_notes)
            .unwrap_or(0);

        if note.tuplet_start && actual_notes > 0 {
            let mut group = vec![i];
            let mut j = i + 1;
            while j < measure.notes.len() {
                let n = &measure.notes[j];
                if n.chord {
                    group.push(j);
                    j += 1;
                    continue;
                }
                if let Some(sf) = staff_filter {
                    if n.staff.unwrap_or(1) != sf {
                        j += 1;
                        continue;
                    }
                }
                group.push(j);
                if n.tuplet_stop {
                    groups.push((group.clone(), actual_notes));
                    i = j + 1;
                    break;
                }
                j += 1;
                if group.len() >= actual_notes as usize {
                    groups.push((group.clone(), actual_notes));
                    i = j;
                    break;
                }
            }
            if j >= measure.notes.len() && !group.is_empty() {
                groups.push((group, actual_notes));
                i = measure.notes.len();
            }
            continue;
        }

        if note.time_modification.is_some() && !note.tuplet_start && !note.tuplet_stop {
            let tm = note.time_modification.as_ref().unwrap();
            let actual = tm.actual_notes as usize;
            let mut group = Vec::new();
            let mut j = i;
            for _ in 0..actual {
                if j >= measure.notes.len() {
                    break;
                }
                let n = &measure.notes[j];
                if n.chord {
                    group.push(j);
                    j += 1;
                    continue;
                }
                if let Some(sf) = staff_filter {
                    if n.staff.unwrap_or(1) != sf {
                        j += 1;
                        continue;
                    }
                }
                if n.time_modification.as_ref().map_or(false, |t| {
                    t.actual_notes == tm.actual_notes && t.normal_notes == tm.normal_notes
                }) {
                    group.push(j);
                    j += 1;
                } else {
                    break;
                }
            }
            if group.len() >= 2 {
                groups.push((group.clone(), tm.actual_notes));
            }
            i = j;
            continue;
        }

        i += 1;
    }
    groups
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{Measure, Note, TimeModification};

    fn note_with_tuplet(actual: i32, normal: i32, tuplet_start: bool, tuplet_stop: bool) -> Note {
        Note {
            pitch: Some(crate::model::Pitch {
                step: "C".to_string(),
                octave: 4,
                alter: None,
            }),
            duration: 1,
            voice: Some(1),
            note_type: Some("eighth".to_string()),
            stem: None,
            beams: vec![],
            rest: false,
            measure_rest: false,
            chord: false,
            dot: false,
            accidental: None,
            tie_start: false,
            tie_stop: false,
            time_modification: Some(TimeModification {
                actual_notes: actual,
                normal_notes: normal,
            }),
            tuplet_start,
            tuplet_stop,
            staff: None,
            default_x: None,
            default_y: None,
            lyrics: vec![],
            grace: false,
            grace_slash: false,
            slurs: vec![],
        }
    }

    fn rest_note() -> Note {
        Note {
            pitch: None,
            duration: 1,
            voice: Some(1),
            note_type: Some("eighth".to_string()),
            stem: None,
            beams: vec![],
            rest: true,
            measure_rest: false,
            chord: false,
            dot: false,
            accidental: None,
            tie_start: false,
            tie_stop: false,
            time_modification: None,
            tuplet_start: false,
            tuplet_stop: false,
            staff: None,
            default_x: None,
            default_y: None,
            lyrics: vec![],
            grace: false,
            grace_slash: false,
            slurs: vec![],
        }
    }

    #[test]
    fn find_tuplet_groups_triplet_with_bracket() {
        let measure = Measure {
            number: 1,
            implicit: false,
            width: None,
            attributes: None,
            notes: vec![
                note_with_tuplet(3, 2, true, false),
                note_with_tuplet(3, 2, false, false),
                note_with_tuplet(3, 2, false, true),
            ],
            harmonies: vec![],
            barlines: vec![],
            directions: vec![],
            new_system: false,
            new_page: false,
        };
        let groups = find_tuplet_groups(&measure, None);
        assert_eq!(groups.len(), 1);
        assert_eq!(groups[0].0, vec![0, 1, 2]);
        assert_eq!(groups[0].1, 3);
    }

    #[test]
    fn find_tuplet_groups_no_tuplets() {
        let measure = Measure {
            number: 1,
            implicit: false,
            width: None,
            attributes: None,
            notes: vec![rest_note(), rest_note()],
            harmonies: vec![],
            barlines: vec![],
            directions: vec![],
            new_system: false,
            new_page: false,
        };
        let groups = find_tuplet_groups(&measure, None);
        assert!(groups.is_empty());
    }

    #[test]
    fn find_tuplet_groups_skips_rests_and_grace() {
        let mut n = note_with_tuplet(3, 2, true, false);
        n.grace = true;
        let measure = Measure {
            number: 1,
            implicit: false,
            width: None,
            attributes: None,
            notes: vec![rest_note(), n],
            harmonies: vec![],
            barlines: vec![],
            directions: vec![],
            new_system: false,
            new_page: false,
        };
        let groups = find_tuplet_groups(&measure, None);
        assert!(groups.is_empty());
    }
}
