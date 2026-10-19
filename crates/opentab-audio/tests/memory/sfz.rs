use std::collections::BTreeMap;
use std::fs;

use opentab_audio::{
    Bank, BankError, SFZ_MAX_EXPANDED_BYTES, SampleRate, SfzBank, SfzLimits, Synth, UnsafeBank,
    decode_audio, parse_sfz_text,
};
use opentab_playback::{Channel, MidiMessage};
use opentab_testkit::banks::{
    ExtraCodebook, FlacOverrides, FlacSpec, SAMPLE_RATE, VorbisSpec, WavOverrides, WavSpec,
    mono_points, samples, wav16, write_flac_with, write_vorbis, write_wav_with,
};
use stats_alloc::Region;

use crate::{GLOBAL, RENDER_FRAMES, SLACK_BYTES, TRACKS, bounded};

const PAD_FRAMES: u32 = 4 * 1024 * 1024;
const PER_TRACK_BYTES: usize = 64 * 1024;
const MIN_DECODED_BYTES: usize = 16_000_000;
const REFUSAL_BYTES: usize = 64 * 1024;
const TEXT_FACTOR: usize = 8;
const FRAGMENT_REGIONS: usize = 200;
const SPARSE_BYTES: u64 = 1 << 30;

fn allocated(region: &Region<'_, std::alloc::System>) -> usize {
    region.change().bytes_allocated
}

fn net(region: &Region<'_, std::alloc::System>) -> usize {
    let change = region.change();
    change
        .bytes_allocated
        .saturating_sub(change.bytes_deallocated)
}

pub fn twenty_tracks_of_one_sfz_bank_decode_each_sample_once() {
    let dir = tempfile::Builder::new()
        .prefix("sfz-memory-")
        .tempdir_in(env!("CARGO_TARGET_TMPDIR"))
        .expect("scratch");
    let root = dir.path().join("bank");
    fs::create_dir_all(&root).expect("bank folder");
    fs::write(
        root.join("pad.sfz"),
        "<region> sample=pad.wav loop_mode=loop_continuous pitch_keycenter=69",
    )
    .expect("sfz");
    fs::write(root.join("pad.wav"), wav16(&samples::long_pad(PAD_FRAMES))).expect("wav");
    let bank = SfzBank::load(&root).expect("loads");
    let channel = Channel::new(0).expect("channel");
    let playing = Region::new(GLOBAL);
    let mut voices: Vec<Box<dyn Synth>> = (0..TRACKS)
        .map(|_| bank.instantiate(SampleRate::STUDIO))
        .collect();
    for voice in &mut voices {
        voice.handle(
            channel,
            &MidiMessage::NoteOn {
                key: 69,
                velocity: 100,
            },
        );
    }
    let added = net(&playing);
    let decoded = usize::try_from(bank.decoded_bytes()).unwrap_or(usize::MAX);
    assert!(decoded >= MIN_DECODED_BYTES, "{decoded} decoded bytes");
    assert_eq!(bank.decode_count(), 1);
    assert_eq!(bank.shared_count(), 1 + TRACKS);
    assert!(
        added <= decoded + PER_TRACK_BYTES * TRACKS,
        "{TRACKS} tracks added {added} bytes for one {decoded}-byte sample"
    );
    let mut left = vec![0.0f32; RENDER_FRAMES];
    let mut right = vec![0.0f32; RENDER_FRAMES];
    let rendering = Region::new(GLOBAL);
    for voice in &mut voices {
        voice.render(&mut left, &mut right);
    }
    let rendered = rendering.change();
    assert!(left.iter().any(|sample| *sample != 0.0));
    assert_eq!(rendered.bytes_allocated, 0, "{rendered:?}");
    drop(voices);
    assert_eq!(bank.shared_count(), 1);
}

fn refused_with_little(bytes: &[u8], reason: UnsafeBank) {
    let decoding = Region::new(GLOBAL);
    let result = decode_audio(bytes, &SfzLimits::DEFAULT).map(|sample| sample.frames);
    let spent = allocated(&decoding);
    assert_eq!(result, Err(BankError::Unsafe { reason }));
    assert!(
        spent <= REFUSAL_BYTES + bytes.len(),
        "{reason:?}: {spent} bytes for {} input bytes",
        bytes.len()
    );
}

pub fn hostile_samples_never_allocate_what_they_declare() {
    let mut tables = VorbisSpec::tone(SAMPLE_RATE, 4, 3);
    tables.extra_codebook = Some(ExtraCodebook {
        entries: (1 << 24) - 1,
        dimensions: 65_535,
        lookup_type: 2,
        value_bits: 32,
    });
    refused_with_little(&write_vorbis(&tables), UnsafeBank::VorbisTables);
    let mut comments = VorbisSpec::tone(SAMPLE_RATE, 4, 3);
    comments.declared_comments = Some(u32::MAX);
    refused_with_little(&write_vorbis(&comments), UnsafeBank::VorbisComments);
    let mut vendor = VorbisSpec::tone(SAMPLE_RATE, 4, 3);
    vendor.declared_vendor_bytes = Some(u32::MAX);
    refused_with_little(&write_vorbis(&vendor), UnsafeBank::VorbisComments);

    let pluck = samples::pluck();
    let points = mono_points(&pluck);
    let flac = write_flac_with(
        &FlacSpec {
            rate: SAMPLE_RATE,
            channels: 1,
            bits: 16,
        },
        &points,
        &FlacOverrides {
            declared_frames: Some((1 << 36) - 1),
            ..FlacOverrides::NONE
        },
    );
    let wav = write_wav_with(
        &WavSpec::mono16(SAMPLE_RATE),
        &points,
        &WavOverrides {
            declared_data_bytes: Some(u32::MAX - 64),
            truncate_to: None,
        },
    );
    let decoded_bytes = points.len() * 4;
    for bytes in [flac, wav] {
        let decoding = Region::new(GLOBAL);
        let _ = decode_audio(&bytes, &SfzLimits::DEFAULT);
        let spent = allocated(&decoding);
        assert!(
            spent <= 2 * decoded_bytes + bounded(bytes.len()),
            "{spent} bytes for {decoded_bytes} decoded bytes"
        );
    }
}

pub fn a_huge_declared_file_is_refused_before_it_is_read() {
    let dir = tempfile::Builder::new()
        .prefix("sfz-sparse-")
        .tempdir_in(env!("CARGO_TARGET_TMPDIR"))
        .expect("scratch");
    let root = dir.path().join("bank");
    fs::create_dir_all(&root).expect("bank folder");
    let file = fs::File::create(root.join("huge.sfz")).expect("sparse file");
    file.set_len(SPARSE_BYTES).expect("sparse length");
    drop(file);
    let loading = Region::new(GLOBAL);
    let result = SfzBank::load(&root).map(|bank| bank.sample_count());
    let spent = allocated(&loading);
    assert_eq!(
        result,
        Err(BankError::Unsafe {
            reason: UnsafeBank::SfzFileTooLarge { limit_kib: 4096 }
        })
    );
    assert!(spent <= REFUSAL_BYTES, "{spent} bytes");
}

pub fn text_bombs_stop_at_the_expansion_cap() {
    let fragment = "<region> sample=pluck.wav lokey=1 hikey=2\n".repeat(FRAGMENT_REGIONS);
    let mut includes = BTreeMap::new();
    includes.insert("f.sfz".to_owned(), fragment);
    let bomb = "#include \"f.sfz\"\n".repeat(4000);
    let parsing = Region::new(GLOBAL);
    let result =
        parse_sfz_text(&bomb, &includes, &SfzLimits::DEFAULT).map(|text| text.regions.len());
    let spent = allocated(&parsing);
    assert!(matches!(
        result,
        Err(BankError::Unsafe {
            reason: UnsafeBank::ExpandedTextTooLarge { .. }
        })
    ));
    let cap = usize::try_from(SFZ_MAX_EXPANDED_BYTES).unwrap_or(usize::MAX);
    assert!(spent <= TEXT_FACTOR * cap + SLACK_BYTES, "{spent} bytes");

    let define = format!(
        "#define $A {}\n<region> sample=a.wav volume={}",
        "9".repeat(60_000),
        "$A".repeat(100_000)
    );
    let parsing = Region::new(GLOBAL);
    let result = parse_sfz_text(&define, &BTreeMap::new(), &SfzLimits::DEFAULT)
        .map(|text| text.regions.len());
    let spent = allocated(&parsing);
    assert!(matches!(
        result,
        Err(BankError::Unsafe {
            reason: UnsafeBank::ExpandedTextTooLarge { .. }
        })
    ));
    assert!(spent <= TEXT_FACTOR * cap + SLACK_BYTES, "{spent} bytes");
}
