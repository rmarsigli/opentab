use std::collections::HashSet;

use serde::{Deserialize, Serialize};

use crate::fraction::checked_sum;
use crate::{ElementId, Fraction, NoteId, NoteKind, Score, Staff, StaffId, Track, Voice, VoiceId};

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum Diagnostic {
    IncompleteBar {
        voice: VoiceId,
        filled: Fraction,
        expected: Fraction,
    },
    OverfullBar {
        voice: VoiceId,
        filled: Fraction,
        expected: Fraction,
    },
    StaffBarCountMismatch {
        staff: StaffId,
        bars: usize,
        master_bars: usize,
    },
    StringOutOfRange {
        note: NoteId,
        string: u8,
        strings: usize,
    },
    DuplicateId {
        id: ElementId,
    },
    IdAboveAllocator {
        id: ElementId,
    },
}

pub fn diagnose(score: &Score) -> Vec<Diagnostic> {
    let mut report = Report {
        seen: HashSet::new(),
        duplicates: HashSet::new(),
        limit: score.ids.peek(),
        diagnostics: Vec::new(),
    };
    for master_bar in &score.master_bars {
        report.id(master_bar.id.0);
    }
    for track in &score.tracks {
        report.id(track.id.0);
        for staff in &track.staves {
            report.staff(score, track, staff);
        }
    }
    report.diagnostics
}

struct Report {
    seen: HashSet<ElementId>,
    duplicates: HashSet<ElementId>,
    limit: ElementId,
    diagnostics: Vec<Diagnostic>,
}

impl Report {
    fn id(&mut self, id: ElementId) {
        if !self.seen.insert(id) {
            if self.duplicates.insert(id) {
                self.diagnostics.push(Diagnostic::DuplicateId { id });
            }
        } else if id >= self.limit {
            self.diagnostics.push(Diagnostic::IdAboveAllocator { id });
        }
    }

    fn staff(&mut self, score: &Score, track: &Track, staff: &Staff) {
        self.id(staff.id.0);
        if staff.bars.len() != score.master_bars.len() {
            self.diagnostics.push(Diagnostic::StaffBarCountMismatch {
                staff: staff.id,
                bars: staff.bars.len(),
                master_bars: score.master_bars.len(),
            });
        }
        for (index, bar) in staff.bars.iter().enumerate() {
            self.id(bar.id.0);
            let expected = score
                .master_bars
                .get(index)
                .map(|master_bar| master_bar.time_signature.bar_length());
            for voice in &bar.voices {
                self.voice(track, voice, expected);
            }
        }
    }

    fn voice(&mut self, track: &Track, voice: &Voice, expected: Option<Fraction>) {
        self.id(voice.id.0);
        for beat in &voice.beats {
            self.id(beat.id.0);
            for note in &beat.notes {
                self.id(note.id.0);
                if let NoteKind::Fretted { string, .. } = note.kind {
                    let strings = track.tuning.strings.len();
                    if usize::from(string) >= strings {
                        self.diagnostics.push(Diagnostic::StringOutOfRange {
                            note: note.id,
                            string,
                            strings,
                        });
                    }
                }
            }
        }
        if voice.beats.is_empty() {
            return;
        }
        let Some(expected) = expected else {
            return;
        };
        let Some(filled) = checked_sum(voice.beats.iter().map(|beat| beat.duration.length()))
        else {
            return;
        };
        if filled < expected {
            self.diagnostics.push(Diagnostic::IncompleteBar {
                voice: voice.id,
                filled,
                expected,
            });
        } else if filled > expected {
            self.diagnostics.push(Diagnostic::OverfullBar {
                voice: voice.id,
                filled,
                expected,
            });
        }
    }
}
