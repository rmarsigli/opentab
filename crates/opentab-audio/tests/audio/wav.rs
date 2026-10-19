use opentab_audio::{RenderedAudio, SampleRate, WAV_HEADER_BYTES, encode_wav, pcm16};

const LSB: f32 = 1.0 / 8_388_608.0;

#[test]
fn pcm16_floors_and_saturates_deterministically() {
    assert_eq!(pcm16(0.0), 0);
    assert_eq!(pcm16(0.5), 16_384);
    assert_eq!(pcm16(-0.5), -16_384);
    assert_eq!(pcm16(1.0), i16::MAX);
    assert_eq!(pcm16(-1.0), i16::MIN);
    assert_eq!(pcm16(3.0), i16::MAX);
    assert_eq!(pcm16(-3.0), i16::MIN);
    assert_eq!(pcm16(f32::INFINITY), i16::MAX);
    assert_eq!(pcm16(f32::NEG_INFINITY), i16::MIN);
    assert_eq!(pcm16(f32::NAN), 0);
    assert_eq!(pcm16(LSB), 0);
    assert_eq!(pcm16(-LSB), -1);
    assert_eq!(pcm16(1.0 / 32_768.0), 1);
    assert_eq!(pcm16(-1.0 / 32_768.0), -1);
    for sample in [
        -8_388_608i32,
        -8_388_353,
        -257,
        -256,
        -1,
        0,
        1,
        255,
        256,
        8_388_607,
    ] {
        let expected = i16::try_from(sample >> 8).expect("fits");
        assert_eq!(pcm16(sample as f32 * LSB), expected, "{sample}");
    }
}

#[test]
fn encode_wav_writes_a_stereo_16_bit_riff_file() {
    let audio = RenderedAudio {
        rate: SampleRate::CD,
        frames: 3,
        left: vec![0.0, 0.5, -1.0],
        right: vec![1.0, -0.5, 0.25],
    };
    let bytes = encode_wav(&audio);
    assert_eq!(bytes.len(), WAV_HEADER_BYTES + 12);
    assert_eq!(&bytes[0..4], b"RIFF");
    assert_eq!(
        u32::from_le_bytes(bytes[4..8].try_into().expect("size")),
        36 + 12
    );
    assert_eq!(&bytes[8..16], b"WAVEfmt ");
    assert_eq!(
        u32::from_le_bytes(bytes[16..20].try_into().expect("fmt")),
        16
    );
    assert_eq!(u16::from_le_bytes([bytes[20], bytes[21]]), 1);
    assert_eq!(u16::from_le_bytes([bytes[22], bytes[23]]), 2);
    assert_eq!(
        u32::from_le_bytes(bytes[24..28].try_into().expect("rate")),
        44_100
    );
    assert_eq!(
        u32::from_le_bytes(bytes[28..32].try_into().expect("bytes")),
        176_400
    );
    assert_eq!(u16::from_le_bytes([bytes[32], bytes[33]]), 4);
    assert_eq!(u16::from_le_bytes([bytes[34], bytes[35]]), 16);
    assert_eq!(&bytes[36..40], b"data");
    assert_eq!(
        u32::from_le_bytes(bytes[40..44].try_into().expect("data")),
        12
    );
    let samples: Vec<i16> = bytes[44..]
        .chunks(2)
        .map(|pair| i16::from_le_bytes([pair[0], pair[1]]))
        .collect();
    assert_eq!(samples, vec![0, 32_767, 16_384, -16_384, -32_768, 8_192]);
}

#[test]
fn encode_wav_never_writes_more_frames_than_the_buffers_hold() {
    let audio = RenderedAudio {
        rate: SampleRate::STUDIO,
        frames: 10,
        left: vec![0.0; 2],
        right: vec![0.0; 4],
    };
    let bytes = encode_wav(&audio);
    assert_eq!(bytes.len(), WAV_HEADER_BYTES + 8);
    assert_eq!(
        u32::from_le_bytes(bytes[40..44].try_into().expect("data")),
        8
    );
}
