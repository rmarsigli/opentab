use opentab_audio::{AudioKind, BankError, SfzLimits, UnsafeBank, audio_kind, decode_audio};
use opentab_testkit::banks::{
    ExtraCodebook, FlacOverrides, FlacSpec, SAMPLE_RATE, SINE_BIN, SINE_PACKETS, VorbisSpec,
    WavOverrides, WavSpec, flac16, mono_points, samples, sine_ogg, stereo_points, wav16,
    write_flac, write_flac_with, write_vorbis, write_wav, write_wav_with,
};

fn expected(points: &[i32], bits: u32) -> Vec<f32> {
    let scale = (1u64 << (bits - 1)) as f32;
    points.iter().map(|point| *point as f32 / scale).collect()
}

fn unsafe_reason(result: Result<opentab_audio::DecodedSample, BankError>) -> UnsafeBank {
    match result {
        Err(BankError::Unsafe { reason }) => reason,
        other => panic!("expected an unsafe refusal, got {other:?}"),
    }
}

fn is_invalid(result: &Result<opentab_audio::DecodedSample, BankError>) -> bool {
    matches!(result, Err(BankError::Invalid { .. }))
}

#[test]
fn a_sixteen_bit_wav_decodes_to_its_exact_points() {
    let pluck = samples::pluck();
    let decoded = decode_audio(&wav16(&pluck), &SfzLimits::DEFAULT).expect("decodes");
    assert_eq!(decoded.rate, SAMPLE_RATE);
    assert_eq!(decoded.channels, 1);
    assert_eq!(decoded.frames, pluck.points.len());
    assert_eq!(decoded.data, expected(&mono_points(&pluck), 16));
}

#[test]
fn an_extensible_twenty_four_bit_stereo_wav_decodes_exactly() {
    let points: Vec<i32> = stereo_points(&samples::saw_bass(), 2)
        .into_iter()
        .map(|point| point << 8)
        .collect();
    let spec = WavSpec {
        rate: 48_000,
        channels: 2,
        bits: 24,
        extensible: true,
    };
    let bytes = write_wav(&spec, &points);
    assert_eq!(audio_kind(&bytes), Some(AudioKind::Wav));
    let decoded = decode_audio(&bytes, &SfzLimits::DEFAULT).expect("decodes");
    assert_eq!((decoded.rate, decoded.channels), (48_000, 2));
    assert_eq!(decoded.data, expected(&points, 24));
}

#[test]
fn a_verbatim_flac_decodes_to_its_exact_points_in_both_depths() {
    let pluck = samples::pluck();
    let decoded = decode_audio(&flac16(&pluck), &SfzLimits::DEFAULT).expect("decodes");
    assert_eq!(decoded.data, expected(&mono_points(&pluck), 16));
    let points: Vec<i32> = stereo_points(&samples::kick(), 2)
        .into_iter()
        .map(|point| point << 8)
        .collect();
    let spec = FlacSpec {
        rate: SAMPLE_RATE,
        channels: 2,
        bits: 24,
    };
    let bytes = write_flac(&spec, &points);
    assert_eq!(audio_kind(&bytes), Some(AudioKind::Flac));
    let decoded = decode_audio(&bytes, &SfzLimits::DEFAULT).expect("decodes");
    assert_eq!(decoded.channels, 2);
    assert_eq!(decoded.data, expected(&points, 24));
}

fn dominant_period(data: &[f32]) -> f64 {
    let crossings: Vec<usize> = data
        .windows(2)
        .enumerate()
        .filter(|(_, pair)| pair[0] <= 0.0 && pair[1] > 0.0)
        .map(|(index, _)| index)
        .collect();
    let first = crossings.first().copied().unwrap_or(0);
    let last = crossings.last().copied().unwrap_or(0);
    (last - first) as f64 / (crossings.len().max(2) - 1) as f64
}

#[test]
fn the_generated_ogg_decodes_to_a_steady_tone_at_its_bin() {
    let bytes = sine_ogg();
    assert_eq!(audio_kind(&bytes), Some(AudioKind::Ogg));
    let decoded = decode_audio(&bytes, &SfzLimits::DEFAULT).expect("decodes");
    assert_eq!(decoded.rate, SAMPLE_RATE);
    assert_eq!(decoded.channels, 1);
    assert_eq!(decoded.frames, (SINE_PACKETS - 1) * 128);
    let peak = decoded.peak();
    assert!(peak > 0.05 && peak < 1.0, "peak {peak}");
    let middle = &decoded.data[1024..decoded.frames - 1024];
    let period = dominant_period(middle);
    let wanted = 256.0 / (SINE_BIN as f64 + 1.0);
    assert!(
        (period - wanted).abs() < 0.5,
        "period {period}, wanted {wanted}"
    );
    for (index, value) in middle[..middle.len() - 128].iter().enumerate() {
        assert!((value - middle[index + 128]).abs() < 1e-4, "frame {index}");
    }
}

#[test]
fn a_stereo_ogg_decodes_two_channels() {
    let mut spec = VorbisSpec::tone(48_000, 12, 9);
    spec.channels = 2;
    let decoded = decode_audio(&write_vorbis(&spec), &SfzLimits::DEFAULT).expect("decodes");
    assert_eq!(
        (decoded.rate, decoded.channels, decoded.frames),
        (48_000, 2, 11 * 128)
    );
    assert!(decoded.peak() > 0.05);
}

#[test]
fn unknown_audio_is_unsupported_and_never_a_panic() {
    for bytes in [&b""[..], b"FORM\0\0\0\0AIFF", b"ID3\x04 this is an mp3"] {
        assert!(matches!(
            decode_audio(bytes, &SfzLimits::DEFAULT),
            Err(BankError::Unsupported { .. })
        ));
    }
}

#[test]
fn truncated_streams_are_invalid() {
    let pluck = samples::pluck();
    let wav = wav16(&pluck);
    let flac = flac16(&pluck);
    let ogg = sine_ogg();
    for (bytes, cut) in [
        (&wav, wav.len() - 3),
        (&flac, flac.len() - 3),
        (&ogg, ogg.len() - 3),
    ] {
        let result = decode_audio(&bytes[..cut], &SfzLimits::DEFAULT);
        assert!(is_invalid(&result), "{result:?}");
    }
    let lying = write_wav_with(
        &WavSpec::mono16(SAMPLE_RATE),
        &mono_points(&pluck),
        &WavOverrides {
            declared_data_bytes: Some(u32::MAX - 64),
            truncate_to: None,
        },
    );
    assert!(is_invalid(&decode_audio(&lying, &SfzLimits::DEFAULT)));
}

#[test]
fn a_flac_declaring_a_huge_length_decodes_only_what_it_holds() {
    let pluck = samples::pluck();
    let spec = FlacSpec {
        rate: SAMPLE_RATE,
        channels: 1,
        bits: 16,
    };
    let overrides = FlacOverrides {
        declared_frames: Some((1u64 << 36) - 1),
        ..FlacOverrides::NONE
    };
    let bytes = write_flac_with(&spec, &mono_points(&pluck), &overrides);
    let decoded = decode_audio(&bytes, &SfzLimits::DEFAULT).expect("decodes");
    assert_eq!(decoded.frames, pluck.points.len());
}

#[test]
fn a_sample_longer_than_the_frame_cap_is_refused() {
    let limits = SfzLimits {
        sample_frames: 1000,
        ..SfzLimits::DEFAULT
    };
    let pluck = samples::pluck();
    for bytes in [wav16(&pluck), flac16(&pluck), sine_ogg()] {
        assert_eq!(
            unsafe_reason(decode_audio(&bytes, &limits)),
            UnsafeBank::SampleTooLong { limit: 1000 }
        );
    }
}

#[test]
fn a_sample_beyond_the_decoded_budget_is_refused() {
    let limits = SfzLimits {
        decoded_bytes: 4 << 10,
        ..SfzLimits::DEFAULT
    };
    for bytes in [
        wav16(&samples::pluck()),
        flac16(&samples::pluck()),
        sine_ogg(),
    ] {
        assert_eq!(
            unsafe_reason(decode_audio(&bytes, &limits)),
            UnsafeBank::DecodedAudioBudget { limit_mib: 0 }
        );
    }
}

#[test]
fn an_audio_file_beyond_the_size_cap_is_refused() {
    let limits = SfzLimits {
        audio_file_bytes: 1024,
        ..SfzLimits::DEFAULT
    };
    assert_eq!(
        unsafe_reason(decode_audio(&wav16(&samples::pluck()), &limits)),
        UnsafeBank::AudioFileTooLarge { limit_mib: 0 }
    );
}

#[test]
fn more_than_two_channels_are_refused_in_every_format() {
    let wav = write_wav(
        &WavSpec {
            rate: SAMPLE_RATE,
            channels: 6,
            bits: 16,
            extensible: false,
        },
        &[0; 60],
    );
    let flac = write_flac(
        &FlacSpec {
            rate: SAMPLE_RATE,
            channels: 6,
            bits: 16,
        },
        &[0; 60],
    );
    let mut spec = VorbisSpec::tone(SAMPLE_RATE, 4, 3);
    spec.channels = 6;
    let ogg = write_vorbis(&spec);
    for bytes in [wav, flac, ogg] {
        assert_eq!(
            unsafe_reason(decode_audio(&bytes, &SfzLimits::DEFAULT)),
            UnsafeBank::SampleChannels { channels: 6 }
        );
    }
}

#[test]
fn rates_outside_the_window_are_refused_in_every_format() {
    let wav = write_wav(&WavSpec::mono16(400_000), &[0; 16]);
    let flac = write_flac(
        &FlacSpec {
            rate: 400_000,
            channels: 1,
            bits: 16,
        },
        &[0; 16],
    );
    let ogg = write_vorbis(&VorbisSpec::tone(400_000, 4, 3));
    for bytes in [wav, flac, ogg] {
        assert_eq!(
            unsafe_reason(decode_audio(&bytes, &SfzLimits::DEFAULT)),
            UnsafeBank::SampleRate { hz: 400_000 }
        );
    }
}

fn with_codebook(entries: u32, dimensions: u16, lookup_type: u8) -> Vec<u8> {
    let mut spec = VorbisSpec::tone(SAMPLE_RATE, 4, 3);
    spec.extra_codebook = Some(ExtraCodebook {
        entries,
        dimensions,
        lookup_type,
        value_bits: 1,
    });
    write_vorbis(&spec)
}

#[test]
fn a_vorbis_codebook_declaring_too_many_entries_is_refused() {
    assert_eq!(
        unsafe_reason(decode_audio(
            &with_codebook((1 << 24) - 1, 1, 0),
            &SfzLimits::DEFAULT
        )),
        UnsafeBank::VorbisTables
    );
}

#[test]
fn a_vorbis_codebook_declaring_huge_lookup_tables_is_refused() {
    for (entries, dimensions, lookup_type) in [(1000, 65_535, 2u8), (1000, 65_535, 1), (4, 0, 1)] {
        assert_eq!(
            unsafe_reason(decode_audio(
                &with_codebook(entries, dimensions, lookup_type),
                &SfzLimits::DEFAULT
            )),
            UnsafeBank::VorbisTables,
            "{entries} {dimensions} {lookup_type}"
        );
    }
}

#[test]
fn a_vorbis_codebook_within_the_cap_still_decodes() {
    let mut spec = VorbisSpec::tone(SAMPLE_RATE, 4, 3);
    spec.extra_codebook = Some(ExtraCodebook {
        entries: 2,
        dimensions: 2,
        lookup_type: 0,
        value_bits: 1,
    });
    assert!(decode_audio(&write_vorbis(&spec), &SfzLimits::DEFAULT).is_ok());
}

#[test]
fn vorbis_comments_declaring_more_than_the_packet_are_refused() {
    let mut vendor = VorbisSpec::tone(SAMPLE_RATE, 4, 3);
    vendor.declared_vendor_bytes = Some(u32::MAX);
    let mut count = VorbisSpec::tone(SAMPLE_RATE, 4, 3);
    count.declared_comments = Some(u32::MAX);
    let mut length = VorbisSpec::tone(SAMPLE_RATE, 4, 3);
    length.declared_comment_bytes = Some(u32::MAX);
    for spec in [vendor, count, length] {
        assert_eq!(
            unsafe_reason(decode_audio(&write_vorbis(&spec), &SfzLimits::DEFAULT)),
            UnsafeBank::VorbisComments
        );
    }
}

#[test]
fn ogg_files_named_by_opentab_ogg_decode_through_the_guard() {
    let Ok(list) = std::env::var("OPENTAB_OGG") else {
        return;
    };
    for path in list.split(':').filter(|path| !path.is_empty()) {
        let bytes = std::fs::read(path).expect("the named OGG file reads");
        let decoded = decode_audio(&bytes, &SfzLimits::DEFAULT)
            .unwrap_or_else(|error| panic!("{path}: {error}"));
        println!(
            "{path}: {} Hz, {} channels, {} frames, peak {:.3}",
            decoded.rate,
            decoded.channels,
            decoded.frames,
            decoded.peak()
        );
        assert!(decoded.frames > 0 && decoded.peak() > 0.0);
    }
}
