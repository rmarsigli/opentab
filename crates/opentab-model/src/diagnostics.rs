use std::collections::HashSet;

use serde::{Deserialize, Serialize};

use crate::fraction::{checked_sum, deserialize_non_negative};
use crate::{
    Bar, BarId, Beat, BeatId, ElementId, Fraction, InstrumentKind, MasterBar, MasterBarId, NoteId,
    NoteKind, Score, Staff, StaffId, Track, TrackId, Voice, VoiceId,
};

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum Diagnostic {
    IncompleteBar {
        voice: VoiceId,
        #[serde(deserialize_with = "deserialize_non_negative")]
        filled: Fraction,
        #[serde(deserialize_with = "deserialize_non_negative")]
        expected: Fraction,
    },
    OverfullBar {
        voice: VoiceId,
        #[serde(deserialize_with = "deserialize_non_negative")]
        filled: Fraction,
        #[serde(deserialize_with = "deserialize_non_negative")]
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
    FillNotComputable {
        voice: VoiceId,
    },
    OrphanExtension {
        id: ElementId,
    },
    NoteKindMismatch {
        note: NoteId,
        track: TrackId,
    },
    BarWithoutVoices {
        bar: BarId,
    },
    DuplicateString {
        beat: BeatId,
        string: u8,
    },
    VoiceSlotOrder {
        bar: BarId,
    },
    TempoChangeOutsideBar {
        master_bar: MasterBarId,
        #[serde(deserialize_with = "deserialize_non_negative")]
        position: Fraction,
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
        report.tempo_changes(master_bar);
    }
    for track in &score.tracks {
        report.id(track.id.0);
        for staff in &track.staves {
            report.staff(score, track, staff);
        }
    }
    for id in score.extensions.0.keys() {
        if !report.seen.contains(id) {
            report
                .diagnostics
                .push(Diagnostic::OrphanExtension { id: *id });
        }
    }
    report.diagnostics
}

#[derive(Clone, Copy)]
struct Expected {
    length: Option<Fraction>,
    pickup: bool,
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
            self.bar_voices(bar);
            let expected = score
                .master_bars
                .get(index)
                .map(|master_bar| master_bar.time_signature.bar_length());
            let pickup = index == 0 && score.anacrusis;
            for voice in &bar.voices {
                self.voice(
                    track,
                    voice,
                    Expected {
                        length: expected,
                        pickup,
                    },
                );
            }
        }
    }

    fn tempo_changes(&mut self, master_bar: &MasterBar) {
        let start = Fraction::from_integer(0);
        let end = master_bar.time_signature.bar_length();
        for change in &master_bar.tempo_changes {
            if change.position < start || change.position >= end {
                self.diagnostics.push(Diagnostic::TempoChangeOutsideBar {
                    master_bar: master_bar.id,
                    position: change.position,
                });
            }
        }
    }

    fn bar_voices(&mut self, bar: &Bar) {
        if bar.voices.is_empty() {
            self.diagnostics
                .push(Diagnostic::BarWithoutVoices { bar: bar.id });
        }
        let ordered = bar.voices.windows(2).all(|pair| match pair {
            [first, second] => first.slot < second.slot,
            _ => true,
        });
        if !ordered {
            self.diagnostics
                .push(Diagnostic::VoiceSlotOrder { bar: bar.id });
        }
    }

    fn note(&mut self, track: &Track, note: NoteId, kind: NoteKind) {
        match (track.instrument.kind, kind) {
            (InstrumentKind::Fretted, NoteKind::Fretted { string, .. }) => {
                let strings = track.tuning.strings.len();
                if usize::from(string) >= strings {
                    self.diagnostics.push(Diagnostic::StringOutOfRange {
                        note,
                        string,
                        strings,
                    });
                }
            }
            (InstrumentKind::Percussion, NoteKind::Percussion { .. }) => {}
            _ => self.diagnostics.push(Diagnostic::NoteKindMismatch {
                note,
                track: track.id,
            }),
        }
    }

    fn strings(&mut self, beat: &Beat) {
        let mut used = HashSet::new();
        let mut reported = HashSet::new();
        for note in &beat.notes {
            if let NoteKind::Fretted { string, .. } = note.kind
                && !used.insert(string)
                && reported.insert(string)
            {
                self.diagnostics.push(Diagnostic::DuplicateString {
                    beat: beat.id,
                    string,
                });
            }
        }
    }

    fn voice(&mut self, track: &Track, voice: &Voice, expected: Expected) {
        self.id(voice.id.0);
        for beat in &voice.beats {
            self.id(beat.id.0);
            for note in &beat.notes {
                self.id(note.id.0);
                self.note(track, note.id, note.kind);
            }
            self.strings(beat);
        }
        let timed = || {
            voice
                .beats
                .iter()
                .filter(|beat| beat.effects.grace.is_none())
        };
        if timed().next().is_none() {
            return;
        }
        let Some(filled) = checked_sum(timed().map(|beat| beat.duration.length())) else {
            self.diagnostics
                .push(Diagnostic::FillNotComputable { voice: voice.id });
            return;
        };
        let pickup = expected.pickup;
        let Some(expected) = expected.length else {
            return;
        };
        if filled < expected {
            if pickup {
                return;
            }
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
