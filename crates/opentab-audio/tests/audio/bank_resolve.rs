use std::fs;

use opentab_audio::{
    BankError, BankFormat, BankManager, BankRef, Fallback, FallbackReason, InstalledBank,
    SampleRate, Session, render_session,
};
use opentab_command::{Command, apply};
use opentab_gp::{read_gp, write_gp};
use opentab_model::{InstrumentFamily, Score, from_json, to_json};
use opentab_playback::Profile;

use crate::bank_manager::{manager_in, sfz_without_sidecar, write_tiny_sf2};
use crate::support::{config, fixture, performance, scratch_dir};

const FIXTURE: &str = "multi-track.gp";

fn families(score: &Score) -> Vec<InstrumentFamily> {
    score
        .tracks
        .iter()
        .map(|track| {
            InstrumentFamily::of(
                track.instrument.kind,
                track.instrument.set,
                track.instrument.midi_program,
            )
        })
        .collect()
}

fn installed(bank: &InstalledBank) -> BankRef {
    BankRef::Installed(bank.id.clone())
}

struct Setup {
    _dir: tempfile::TempDir,
    manager: BankManager,
    sf2: InstalledBank,
    sfz: InstalledBank,
    score: Score,
}

fn setup() -> Setup {
    let dir = scratch_dir();
    let sf2_path = write_tiny_sf2(dir.path(), "tiny.sf2");
    let sfz_path = sfz_without_sidecar(dir.path());
    let mut manager = manager_in(dir.path());
    let sf2 = manager.import(&sf2_path).expect("sf2");
    let sfz = manager.import(&sfz_path).expect("sfz");
    let score = fixture(FIXTURE);
    assert!(score.tracks.len() >= 2);
    Setup {
        _dir: dir,
        manager,
        sf2,
        sfz,
        score,
    }
}

#[test]
fn without_preferences_every_track_plays_built_in_in_track_order() {
    let setup = setup();
    let plan = setup.manager.resolve(&setup.score);
    let ids: Vec<_> = plan.tracks.iter().map(|sound| sound.track).collect();
    let expected: Vec<_> = setup.score.tracks.iter().map(|track| track.id).collect();
    assert_eq!(ids, expected);
    assert_eq!(
        plan.tracks
            .iter()
            .map(|sound| sound.family)
            .collect::<Vec<_>>(),
        families(&setup.score)
    );
    assert!(
        plan.tracks
            .iter()
            .all(|sound| sound.bank == BankRef::BuiltIn)
    );
    assert!(plan.tracks.iter().all(|sound| sound.fallback.is_none()));
    let loaded = setup.manager.load_voices(&plan, SampleRate::STUDIO);
    assert_eq!(loaded.voices.len(), setup.score.tracks.len());
    assert!(
        loaded
            .backends
            .iter()
            .all(|backend| backend.format == BankFormat::BuiltIn)
    );
}

#[test]
fn a_family_preference_picks_the_bank_and_persists() {
    let dir_root;
    let plan;
    {
        let mut setup = setup();
        let family = families(&setup.score)[0];
        setup
            .manager
            .assign(family, installed(&setup.sf2))
            .expect("assigns");
        plan = setup.manager.resolve(&setup.score);
        for (sound, track_family) in plan.tracks.iter().zip(families(&setup.score)) {
            let expected = if track_family == family {
                installed(&setup.sf2)
            } else {
                BankRef::BuiltIn
            };
            assert_eq!(sound.bank, expected);
        }
        dir_root = setup.manager.root().to_path_buf();
        let reopened = BankManager::with_catalogue(&dir_root, Vec::new()).expect("reopens");
        assert_eq!(reopened.resolve(&setup.score), plan);
        assert_eq!(
            reopened.assignments().get(&family),
            Some(&installed(&setup.sf2))
        );
        let loaded = reopened.load_voices(&plan, SampleRate::STUDIO);
        assert_eq!(loaded.backends[0].format, BankFormat::Sf2);
        assert_eq!(loaded.backends[0].bank, setup.sf2.id.as_str());
        assert_eq!(loaded.backends[0].fallback, None);
        let rendered_tracks = performance(&setup.score, Profile::Musical);
        let mut session = Session::new(rendered_tracks, loaded.voices, config(SampleRate::STUDIO))
            .expect("a session");
        let audio = render_session(&mut session, &[], 4_800).expect("renders");
        assert_eq!(audio.frames, 4_800);
    }
}

#[test]
fn assigning_a_bank_that_is_not_installed_is_an_error() {
    let mut setup = setup();
    let missing = BankRef::parse("nothing-here").expect("a name");
    assert_eq!(
        setup.manager.assign(InstrumentFamily::Guitar, missing),
        Err(BankError::NotInstalled {
            id: String::from("nothing-here")
        })
    );
    setup
        .manager
        .assign(InstrumentFamily::Guitar, BankRef::BuiltIn)
        .expect("built-in is always assignable");
}

#[test]
fn a_removed_or_deleted_bank_falls_back_and_is_reported() {
    let mut setup = setup();
    let family = families(&setup.score)[0];
    setup
        .manager
        .assign(family, installed(&setup.sf2))
        .expect("assigns");
    fs::remove_dir_all(setup.sf2.path.parent().expect("folder")).expect("deleted by hand");
    let plan = setup.manager.resolve(&setup.score);
    assert_eq!(plan.tracks[0].bank, BankRef::BuiltIn);
    assert_eq!(
        plan.tracks[0].fallback,
        Some(Fallback {
            wanted: setup.sf2.id.to_string(),
            reason: FallbackReason::FilesMissing,
        })
    );
    let loaded = setup.manager.load_voices(&plan, SampleRate::STUDIO);
    assert_eq!(loaded.backends[0].format, BankFormat::BuiltIn);
    assert!(
        loaded.backends[0]
            .fallback
            .as_deref()
            .is_some_and(|text| text.contains(setup.sf2.id.as_str()))
    );
    setup.manager.remove(&setup.sf2.id).expect("removes");
    let plan = setup.manager.resolve(&setup.score);
    assert_eq!(plan.tracks[0].bank, BankRef::BuiltIn);
    assert_eq!(
        plan.tracks[0]
            .fallback
            .as_ref()
            .map(|fallback| &fallback.reason),
        Some(&FallbackReason::NotInstalled)
    );
}

fn with_override(score: &Score, index: usize, bank: &str) -> Score {
    let mut score = score.clone();
    let track = score.tracks[index].id;
    apply(
        &mut score,
        &Command::SetSoundOverride {
            track,
            bank: Some(bank.to_owned()),
        },
    )
    .expect("the override applies");
    score
}

#[test]
fn the_per_score_override_wins_and_round_trips_through_json_and_gp() {
    let mut setup = setup();
    for family in families(&setup.score) {
        setup
            .manager
            .assign(family, installed(&setup.sf2))
            .expect("assigns");
    }
    let score = with_override(&setup.score, 1, setup.sfz.id.as_str());
    let from_text = from_json(&to_json(&score).expect("json")).expect("parses");
    let from_gp = read_gp(&write_gp(&score).expect("gp")).expect("reads");
    for copy in [&score, &from_text, &from_gp] {
        assert_eq!(copy.sound_overrides, score.sound_overrides);
        let plan = setup.manager.resolve(copy);
        assert_eq!(plan.tracks[0].bank, installed(&setup.sf2));
        assert_eq!(plan.tracks[1].bank, installed(&setup.sfz));
        assert!(plan.tracks.iter().all(|sound| sound.fallback.is_none()));
        let loaded = setup.manager.load_voices(&plan, SampleRate::STUDIO);
        assert_eq!(loaded.backends[1].format, BankFormat::Sfz);
    }
    let built_in = with_override(&setup.score, 0, "built-in");
    assert_eq!(
        setup.manager.resolve(&built_in).tracks[0].bank,
        BankRef::BuiltIn
    );
}

#[test]
fn a_missing_override_falls_back_to_the_preference_and_is_reported() {
    let mut setup = setup();
    let family = families(&setup.score)[0];
    setup
        .manager
        .assign(family, installed(&setup.sf2))
        .expect("assigns");
    let missing = with_override(&setup.score, 0, "not-installed");
    let plan = setup.manager.resolve(&missing);
    assert_eq!(plan.tracks[0].bank, installed(&setup.sf2));
    assert_eq!(
        plan.tracks[0].fallback,
        Some(Fallback {
            wanted: String::from("not-installed"),
            reason: FallbackReason::NotInstalled,
        })
    );
    let bad = with_override(&setup.score, 0, "Not A Name");
    let plan = setup.manager.resolve(&bad);
    assert_eq!(plan.tracks[0].bank, installed(&setup.sf2));
    assert_eq!(
        plan.tracks[0]
            .fallback
            .as_ref()
            .map(|fallback| &fallback.reason),
        Some(&FallbackReason::BadName)
    );
}

#[test]
fn an_unloadable_bank_plays_built_in_and_says_why() {
    let mut setup = setup();
    let family = families(&setup.score)[0];
    setup
        .manager
        .assign(family, installed(&setup.sf2))
        .expect("assigns");
    fs::write(&setup.sf2.path, b"RIFF\x04\x00\x00\x00sfbk").expect("damaged");
    let plan = setup.manager.resolve(&setup.score);
    assert_eq!(plan.tracks[0].bank, installed(&setup.sf2));
    let loaded = setup.manager.load_voices(&plan, SampleRate::STUDIO);
    assert_eq!(loaded.voices.len(), setup.score.tracks.len());
    assert_eq!(loaded.backends[0].format, BankFormat::BuiltIn);
    assert!(
        loaded.backends[0]
            .fallback
            .as_deref()
            .is_some_and(|text| text.contains("cannot be used"))
    );
}

#[test]
fn a_registry_path_leaving_the_bank_folder_is_never_used() {
    let mut setup = setup();
    let family = families(&setup.score)[0];
    setup
        .manager
        .assign(family, installed(&setup.sf2))
        .expect("assigns");
    let registry_path = setup.manager.root().join(opentab_audio::REGISTRY_NAME);
    let text = fs::read_to_string(&registry_path).expect("registry");
    let outside = setup.manager.root().join("outside.sf2");
    fs::copy(&setup.sf2.path, &outside).expect("a bank outside");
    let stored = format!("banks/{}/tiny.sf2", setup.sf2.id);
    assert!(text.contains(&stored), "{text}");
    for tampered in [
        format!("banks/{}/../../outside.sf2", setup.sf2.id),
        String::from("outside.sf2"),
        format!("banks/other/../{}/tiny.sf2", setup.sf2.id),
    ] {
        fs::write(&registry_path, text.replace(&stored, &tampered)).expect("tampered");
        let reopened =
            BankManager::with_catalogue(setup.manager.root(), Vec::new()).expect("opens");
        let plan = reopened.resolve(&setup.score);
        assert_eq!(plan.tracks[0].bank, BankRef::BuiltIn, "{tampered}");
        assert_eq!(
            plan.tracks[0]
                .fallback
                .as_ref()
                .map(|fallback| &fallback.reason),
            Some(&FallbackReason::FilesMissing),
            "{tampered}"
        );
    }
}
