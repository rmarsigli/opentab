use opentab_model::{
    Bar, Beat, BeatId, Clef, Fraction, IdAllocator, InstrumentKind, Note, NoteKind, Score, Tempo,
    TempoChange, TimeSignature, Track, Voice, VoiceSlot,
};

use crate::CommandError;
use crate::ids::next_id;

pub(crate) struct Owner<'t> {
    pub track: &'t Track,
    pub articulations: Option<usize>,
}

impl<'t> Owner<'t> {
    pub fn of(score: &Score, track: &'t Track) -> Owner<'t> {
        Owner {
            track,
            articulations: score.articulation_count(track),
        }
    }
}

pub(crate) fn check_note_in(
    owner: &Owner<'_>,
    beat: BeatId,
    notes: &[Note],
    kind: NoteKind,
) -> Result<(), CommandError> {
    let track = owner.track;
    match (track.instrument.kind, kind) {
        (InstrumentKind::Fretted, NoteKind::Fretted { string, .. }) => {
            let strings = track.tuning.strings.len();
            if usize::from(string) >= strings {
                return Err(CommandError::invalid(format!(
                    "string {string} is out of range for track {} with {strings} strings",
                    track.id.0.0
                )));
            }
            let occupied = notes.iter().any(
                |note| matches!(note.kind, NoteKind::Fretted { string: used, .. } if used == string),
            );
            if occupied {
                return Err(CommandError::invalid(format!(
                    "beat {} already has a note on string {string}",
                    beat.0.0
                )));
            }
            Ok(())
        }
        (InstrumentKind::Percussion, NoteKind::Percussion { articulation }) => {
            check_articulation(track, owner.articulations, articulation)
        }
        (InstrumentKind::Percussion, NoteKind::Fretted { .. }) => {
            Err(CommandError::WrongInstrument { track: track.id })
        }
        (InstrumentKind::Fretted, NoteKind::Percussion { .. }) => {
            Err(CommandError::invalid(format!(
                "a percussion note does not belong on fretted track {}",
                track.id.0.0
            )))
        }
    }
}

pub(crate) fn check_articulation(
    track: &Track,
    articulations: Option<usize>,
    articulation: u16,
) -> Result<(), CommandError> {
    let Some(size) = articulations else {
        return Ok(());
    };
    if usize::from(articulation) >= size {
        return Err(CommandError::invalid(format!(
            "articulation {articulation} is outside the drum kit of track {}, which has {size} articulations",
            track.id.0.0
        )));
    }
    Ok(())
}

pub(crate) fn already_broken(error: CommandError) -> CommandError {
    CommandError::invalid(format!(
        "{error}; the score already breaks this rule, so the edit could not be undone; run diagnose"
    ))
}

pub(crate) fn check_trill(track: &Track, note: &Note) -> Result<(), CommandError> {
    let Some(trill) = note.effects.trill else {
        return Ok(());
    };
    if track.instrument.kind == InstrumentKind::Percussion {
        return Err(CommandError::WrongInstrument { track: track.id });
    }
    if !track.trill_fits(note) {
        return Err(CommandError::invalid(format!(
            "the trill's auxiliary fret {} on note {} sounds outside MIDI 0..=127",
            trill.fret, note.id.0.0
        )));
    }
    Ok(())
}

pub(crate) fn check_beat(owner: &Owner<'_>, beat: &Beat) -> Result<(), CommandError> {
    for (index, note) in beat.notes.iter().enumerate() {
        let before = beat.notes.get(..index).unwrap_or_default();
        check_note_in(owner, beat.id, before, note.kind)?;
        check_trill(owner.track, note)?;
    }
    Ok(())
}

pub(crate) fn check_bar(owner: &Owner<'_>, bar: &Bar) -> Result<(), CommandError> {
    let ordered = bar.voices.windows(2).all(|pair| match pair {
        [first, second] => first.slot < second.slot,
        _ => true,
    });
    if !ordered {
        return Err(CommandError::invalid(format!(
            "bar {} has voice slots that are not strictly increasing",
            bar.id.0.0
        )));
    }
    for voice in &bar.voices {
        for beat in &voice.beats {
            check_beat(owner, beat)?;
        }
    }
    Ok(())
}

pub(crate) fn check_empty_bar(bar: &Bar) -> Result<(), CommandError> {
    if bar.voices.iter().any(|voice| !voice.beats.is_empty()) {
        return Err(CommandError::invalid(format!(
            "bar {} holds beats; only empty bars can be trimmed or restored at the end of a staff",
            bar.id.0.0
        )));
    }
    Ok(())
}

pub(crate) fn empty_bar(ids: &mut IdAllocator, clef: Clef) -> Result<Bar, CommandError> {
    Ok(Bar {
        id: next_id(ids)?,
        clef,
        voices: vec![Voice {
            id: next_id(ids)?,
            slot: VoiceSlot::default(),
            beats: Vec::new(),
        }],
    })
}

pub(crate) fn clef_for_new_bar(kind: InstrumentKind, bars: &[Bar], index: usize) -> Clef {
    let neighbour = index
        .checked_sub(1)
        .and_then(|previous| bars.get(previous))
        .or_else(|| bars.get(index));
    match (neighbour, kind) {
        (Some(bar), _) => bar.clef,
        (None, InstrumentKind::Percussion) => Clef::Neutral,
        (None, InstrumentKind::Fretted) => Clef::Treble,
    }
}

pub(crate) fn check_tempo_changes(
    time_signature: TimeSignature,
    tempo_changes: &[TempoChange],
) -> Result<(), CommandError> {
    let start = Fraction::from_integer(0);
    let end = time_signature.bar_length();
    for change in tempo_changes {
        check_exact(change.bpm)?;
        let position = change.position;
        if *position.denom() <= 0 || position < start || position >= end {
            return Err(CommandError::invalid(format!(
                "tempo change position {}/{} is outside the bar of length {}/{}",
                position.numer(),
                position.denom(),
                end.numer(),
                end.denom()
            )));
        }
    }
    Ok(())
}

pub(crate) fn check_exact(tempo: Tempo) -> Result<(), CommandError> {
    if tempo.exact().is_none() {
        return Err(CommandError::invalid(format!(
            "tempo {} has more than six decimals, so it has no exact value",
            tempo.bpm()
        )));
    }
    Ok(())
}
