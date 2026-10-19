use std::collections::HashSet;

use serde::{Deserialize, Serialize};

use crate::fraction::{checked_sum, deserialize_non_negative};
use crate::{
    AutomationValue, Bar, BarId, Beat, BeatId, DirectionJump, DirectionTarget, ElementId, Fraction,
    InstrumentKind, JumpDestination, MasterBar, MasterBarId, NoteId, NoteKind, Score, SoundRef,
    Staff, StaffId, Track, TrackId, Voice, VoiceId, automations_in_order, check_program,
    on_automation_grid,
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
    InexactTempo {
        master_bar: MasterBarId,
        #[serde(deserialize_with = "deserialize_non_negative")]
        position: Fraction,
    },
    AutomationOutsideBar {
        track: TrackId,
        master_bar: MasterBarId,
        #[serde(deserialize_with = "deserialize_non_negative")]
        position: Fraction,
    },
    MissingSound {
        track: TrackId,
        sound: SoundRef,
    },
    FirstSoundMismatch {
        track: TrackId,
    },
    InstrumentSetMismatch {
        track: TrackId,
    },
    SoundOverrideWithoutTrack {
        track: TrackId,
    },
    AutomationOffGrid {
        track: TrackId,
        master_bar: MasterBarId,
        #[serde(deserialize_with = "deserialize_non_negative")]
        position: Fraction,
    },
    AmbiguousSound {
        track: TrackId,
        sound: SoundRef,
    },
    DuplicateSoundOverride {
        track: TrackId,
    },
    ProgramOutOfRange {
        track: TrackId,
    },
    AutomationOrder {
        track: TrackId,
    },
    DanglingJump {
        master_bar: MasterBarId,
        jump: DirectionJump,
        target: DirectionTarget,
    },
    DuplicateTarget {
        master_bar: MasterBarId,
        target: DirectionTarget,
    },
    CodaNotAhead {
        master_bar: MasterBarId,
        jump: DirectionJump,
        target: DirectionTarget,
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
    report.directions(&score.master_bars);
    for track in &score.tracks {
        report.id(track.id.0);
        report.instrument(track);
        report.automations(score, track);
        for staff in &track.staves {
            report.staff(score, track, staff);
        }
    }
    for (index, entry) in score.sound_overrides.iter().enumerate() {
        if score.find_track(entry.track).is_none() {
            report
                .diagnostics
                .push(Diagnostic::SoundOverrideWithoutTrack { track: entry.track });
        }
        if score.sound_overrides[..index]
            .iter()
            .any(|other| other.track == entry.track)
        {
            report
                .diagnostics
                .push(Diagnostic::DuplicateSoundOverride { track: entry.track });
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
            if change.bpm.exact().is_none() {
                self.diagnostics.push(Diagnostic::InexactTempo {
                    master_bar: master_bar.id,
                    position: change.position,
                });
            }
        }
    }

    fn directions(&mut self, master_bars: &[MasterBar]) {
        let mut placed: Vec<DirectionTarget> = Vec::new();
        for master_bar in master_bars {
            for target in &master_bar.directions.targets {
                if placed.contains(target) {
                    self.diagnostics.push(Diagnostic::DuplicateTarget {
                        master_bar: master_bar.id,
                        target: *target,
                    });
                } else {
                    placed.push(*target);
                }
            }
        }
        for (index, master_bar) in master_bars.iter().enumerate() {
            for jump in &master_bar.directions.jumps {
                for target in jump.needs() {
                    if !placed.contains(&target) {
                        self.diagnostics.push(Diagnostic::DanglingJump {
                            master_bar: master_bar.id,
                            jump: *jump,
                            target,
                        });
                    }
                }
                let JumpDestination::Target(target) = jump.destination() else {
                    continue;
                };
                let ahead = master_bars
                    .iter()
                    .skip(index + 1)
                    .any(|later| later.directions.holds(target));
                if !jump.is_return() && placed.contains(&target) && !ahead {
                    self.diagnostics.push(Diagnostic::CodaNotAhead {
                        master_bar: master_bar.id,
                        jump: *jump,
                        target,
                    });
                }
            }
        }
    }

    fn instrument(&mut self, track: &Track) {
        let instrument = &track.instrument;
        if instrument
            .set
            .is_some_and(|set| set.kind() != instrument.kind)
        {
            self.diagnostics
                .push(Diagnostic::InstrumentSetMismatch { track: track.id });
        }
        let programs = std::iter::once(instrument.midi_program)
            .chain(track.sounds.iter().map(|sound| sound.program));
        if programs
            .into_iter()
            .any(|program| check_program(program).is_err())
        {
            self.diagnostics
                .push(Diagnostic::ProgramOutOfRange { track: track.id });
        }
        if let Some(first) = track.sounds.first()
            && (first.program != instrument.midi_program || first.bank != track.midi.bank)
        {
            self.diagnostics
                .push(Diagnostic::FirstSoundMismatch { track: track.id });
        }
    }

    fn automations(&mut self, score: &Score, track: &Track) {
        if !automations_in_order(&track.automations) {
            self.diagnostics
                .push(Diagnostic::AutomationOrder { track: track.id });
        }
        let start = Fraction::from_integer(0);
        for automation in &track.automations {
            let inside = score
                .find_master_bar_index(automation.master_bar)
                .and_then(|index| score.master_bars.get(index))
                .is_some_and(|master_bar| {
                    let position = automation.position;
                    position >= start && position <= master_bar.time_signature.bar_length()
                });
            if !inside {
                self.diagnostics.push(Diagnostic::AutomationOutsideBar {
                    track: track.id,
                    master_bar: automation.master_bar,
                    position: automation.position,
                });
            } else if !on_automation_grid(automation.position) {
                self.diagnostics.push(Diagnostic::AutomationOffGrid {
                    track: track.id,
                    master_bar: automation.master_bar,
                    position: automation.position,
                });
            }
            if let AutomationValue::Sound(sound) = automation.value {
                match track.sounds.get(sound.0) {
                    None => self.diagnostics.push(Diagnostic::MissingSound {
                        track: track.id,
                        sound,
                    }),
                    Some(named) => {
                        let twins = track
                            .sounds
                            .iter()
                            .filter(|other| other.name == named.name)
                            .count();
                        if twins > 1 {
                            self.diagnostics.push(Diagnostic::AmbiguousSound {
                                track: track.id,
                                sound,
                            });
                        }
                    }
                }
            }
        }
    }

    fn bar_voices(&mut self, bar: &Bar) {
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
