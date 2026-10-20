use std::collections::BTreeMap;
use std::fs;

use opentab_audio::{
    Bank, BankError, SFZ_MAX_BANK_INCLUDES, SFZ_MAX_EXPANDED_BYTES, SFZ_MAX_VORBIS_ENTRIES,
    SampleRate, SfzBank, SfzLimits, SoundUse, Synth, UnsafeBank, decode_audio, parse_sfz_text,
    vorbis_setup_cost,
};
use opentab_model::MidiBank;
use opentab_playback::{Channel, MidiMessage};
use opentab_testkit::banks::{
    ExtraCodebook, FilledComments, FlacOverrides, FlacSpec, SAMPLE_RATE, VorbisSpec, WavOverrides,
    WavSpec, mono_points, ogg_start_pages, samples, wav16, write_flac_with, write_vorbis,
    write_wav_with,
};
use stats_alloc::Region;

use crate::{GLOBAL, RENDER_FRAMES, SLACK_BYTES, TRACKS, bounded};

const PAD_FRAMES: u32 = 4 * 1024 * 1024;
const PER_TRACK_BYTES: usize = 64 * 1024;
const MIN_DECODED_BYTES: usize = 16_000_000;
const REFUSAL_BYTES: usize = 64 * 1024;
const TEXT_FACTOR: usize = 8;
const FRAGMENT_REGIONS: usize = 200;
const HUFFMAN_BOMB_DEPTH: u8 = 17;
const HUFFMAN_ACCEPTED_DEPTH: u8 = 14;
const NODE_BYTES: usize = 32;
const COMMENT_BOMB: u32 = 30_000;
const SERIAL_BOMB: u32 = 10_000;
const INCLUDE_LINES: usize = 100;
const INCLUDE_ROOTS: usize = 200;
const BYTES_PER_INCLUDE: usize = 4096;
const PACKET_COPIES: usize = 2;
const SPARSE_BYTES: u64 = 1 << 30;
const SHARED_REGION_LINES: usize = 4;
const REGIONS_PER_LINE: usize = 8000;
const SHARED_SAMPLE_BYTES: usize = 1023;
const BYTES_PER_SHARED_REGION: usize = 64;

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
    let uses = [SoundUse {
        channel,
        bank: MidiBank::default(),
        program: 0,
    }];
    for voice in &mut voices {
        voice.prepare(&uses);
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
        spent <= REFUSAL_BYTES + PACKET_COPIES * bytes.len(),
        "{reason:?}: {spent} bytes for {} input bytes",
        bytes.len()
    );
}

pub fn hostile_samples_never_allocate_what_they_declare() {
    let mut tables = VorbisSpec::tone(SAMPLE_RATE, 4, 3);
    tables.extra_codebook = Some(ExtraCodebook {
        entries: (1 << 24) - 1,
        length: 1,
        dimensions: 65_535,
        lookup_type: 2,
        value_bits: 32,
    });
    refused_with_little(
        &write_vorbis(&tables),
        UnsafeBank::VorbisEntries {
            limit: SFZ_MAX_VORBIS_ENTRIES,
        },
    );
    refused_with_little(&huffman_ogg(HUFFMAN_BOMB_DEPTH), UnsafeBank::VorbisTables);
    let mut comments = VorbisSpec::tone(SAMPLE_RATE, 4, 3);
    comments.declared_comments = Some(u32::MAX);
    refused_with_little(&write_vorbis(&comments), UnsafeBank::VorbisComments);
    let mut many = VorbisSpec::tone(SAMPLE_RATE, 4, 3);
    many.filled_comments = Some(FilledComments {
        count: COMMENT_BOMB,
        length: 0,
    });
    refused_with_little(&write_vorbis(&many), UnsafeBank::VorbisComments);
    refused_with_little(&ogg_start_pages(SERIAL_BOMB), UnsafeBank::OggStreams);
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

fn huffman_ogg(depth: u8) -> Vec<u8> {
    let mut spec = VorbisSpec::tone(SAMPLE_RATE, 4, 3);
    spec.extra_codebook = Some(ExtraCodebook {
        entries: 1 << depth,
        length: depth,
        dimensions: 1,
        lookup_type: 0,
        value_bits: 1,
    });
    write_vorbis(&spec)
}

pub fn the_vorbis_setup_estimate_covers_what_lewton_allocates() {
    let bytes = huffman_ogg(HUFFMAN_ACCEPTED_DEPTH);
    let cost = vorbis_setup_cost(&bytes, &SfzLimits::DEFAULT).expect("within the budget");
    let at_cap = SfzLimits {
        vorbis_setup_bytes: cost.bytes,
        ..SfzLimits::DEFAULT
    };
    let below = SfzLimits {
        vorbis_setup_bytes: cost.bytes - 1,
        ..SfzLimits::DEFAULT
    };
    assert_eq!(
        decode_audio(&bytes, &below).map(|sample| sample.frames),
        Err(BankError::Unsafe {
            reason: UnsafeBank::VorbisTables
        })
    );
    let decoding = Region::new(GLOBAL);
    let decoded = decode_audio(&bytes, &at_cap).expect("decodes at the cap");
    let spent = allocated(&decoding);
    let audio = usize::try_from(decoded.bytes()).unwrap_or(usize::MAX);
    let estimate = usize::try_from(cost.bytes).unwrap_or(usize::MAX);
    println!("vorbis setup estimate {estimate} bytes, decode allocated {spent} bytes");
    assert!(
        spent <= estimate + 2 * audio + SLACK_BYTES,
        "{spent} bytes allocated for an estimate of {estimate}"
    );
    let tree = (1usize << HUFFMAN_ACCEPTED_DEPTH) * 2 * NODE_BYTES;
    assert!(spent >= tree, "{spent} bytes, the tree alone needs {tree}");
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
        "#define $A {}\n{}",
        "9".repeat(60_000),
        format!("<region> sample=a.wav volume={}\n", "$A".repeat(30_000)).repeat(10)
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

const BOMB_FILES: usize = 2_000;
const BOMB_DEPTH: u32 = 6;
const BYTES_PER_WALKED_ENTRY: usize = 512;

pub fn symlink_bombs_are_refused_before_listing() {
    let dir = tempfile::Builder::new()
        .prefix("sfz-listing-")
        .tempdir_in(env!("CARGO_TARGET_TMPDIR"))
        .expect("scratch");
    let root = dir.path().join("bank");
    fs::create_dir_all(&root).expect("bank folder");
    for index in 0..BOMB_FILES {
        fs::write(root.join(format!("f{index:06}")), b"").expect("entry");
    }
    let regions: String = (0..1u32 << BOMB_DEPTH)
        .map(|combo| {
            let path: Vec<&str> = (0..BOMB_DEPTH)
                .map(|bit| if combo >> bit & 1 == 0 { "a" } else { "b" })
                .collect();
            format!("<region> sample={}/missing.wav\n", path.join("/"))
        })
        .collect();
    fs::write(root.join("bomb.sfz"), regions).expect("bomb");
    #[cfg(unix)]
    for name in ["a", "b"] {
        std::os::unix::fs::symlink(".", root.join(name)).expect("link");
    }
    let loading = Region::new(GLOBAL);
    let result = SfzBank::load(&root).map(|bank| bank.listed_entries());
    let spent = allocated(&loading);
    if cfg!(unix) {
        assert_eq!(
            result,
            Err(BankError::Unsafe {
                reason: UnsafeBank::BankSymlink
            })
        );
    }
    let bound = BOMB_FILES * BYTES_PER_WALKED_ENTRY + REFUSAL_BYTES;
    assert!(spent <= bound, "{spent} bytes, bound {bound}");
}

pub fn include_bombs_stop_at_the_bank_include_cap() {
    let dir = tempfile::Builder::new()
        .prefix("sfz-includes-")
        .tempdir_in(env!("CARGO_TARGET_TMPDIR"))
        .expect("scratch");
    let root = dir.path().join("bank");
    fs::create_dir_all(&root).expect("bank folder");
    fs::write(root.join("e.txt"), "").expect("empty");
    fs::write(
        root.join("inc.txt"),
        "#include \"e.txt\"\n".repeat(INCLUDE_LINES),
    )
    .expect("list");
    for index in 0..INCLUDE_ROOTS {
        fs::write(
            root.join(format!("r{index:04}.sfz")),
            "#include \"inc.txt\"\n",
        )
        .expect("root");
    }
    let loading = Region::new(GLOBAL);
    let result = SfzBank::load(&root).map(|bank| bank.sample_count());
    let spent = allocated(&loading);
    assert_eq!(
        result,
        Err(BankError::Unsafe {
            reason: UnsafeBank::BankIncludes {
                limit: SFZ_MAX_BANK_INCLUDES
            }
        })
    );
    let cap = usize::try_from(SFZ_MAX_BANK_INCLUDES).unwrap_or(usize::MAX);
    assert!(spent <= cap * BYTES_PER_INCLUDE, "{spent} bytes");
}

pub fn an_inherited_sample_is_shared_not_copied_per_region() {
    let text = format!(
        "#define $r {}\n<group> sample=*{}\n{}",
        "<region>".repeat(REGIONS_PER_LINE),
        "a".repeat(SHARED_SAMPLE_BYTES),
        "$r\n".repeat(SHARED_REGION_LINES)
    );
    let regions = SHARED_REGION_LINES * REGIONS_PER_LINE;
    let parsing = Region::new(GLOBAL);
    let result = parse_sfz_text(&text, &BTreeMap::new(), &SfzLimits::DEFAULT);
    let spent = allocated(&parsing);
    let parsed = result.expect("every region is a listed generator");
    assert!(parsed.regions.is_empty());
    let expanded = parsed.expanded_bytes;
    let bound = TEXT_FACTOR * expanded + BYTES_PER_SHARED_REGION * regions + SLACK_BYTES;
    let copies = regions * SHARED_SAMPLE_BYTES;
    println!("shared sample: {spent} bytes allocated for {regions} regions, bound {bound}");
    assert!(
        spent <= bound,
        "{spent} bytes for {regions} regions over {expanded} expanded bytes"
    );
    assert!(bound < copies, "the bound {bound} must sit below {copies}");
}
