const VORBIS: &[u8; 6] = b"vorbis";
const IDENT_TYPE: u8 = 1;
const COMMENT_TYPE: u8 = 3;
const SETUP_TYPE: u8 = 5;
const CODEBOOK_SYNC: u64 = 0x56_4342;
const FLOAT_BIAS: u64 = 788;
const FLOOR_ONE: u64 = 1;
const RESIDUE_ONE: u64 = 1;
const FLOOR_MULTIPLIER: u64 = 4;
const FLOOR_Y_BITS: u32 = 6;
const BEGIN_OF_STREAM: u8 = 0x02;
const END_OF_STREAM: u8 = 0x04;
const SEGMENT_BYTES: usize = 255;
const PACKETS_PER_PAGE: usize = 64;
const CRC_POLY: u32 = 0x04C1_1DB7;
const VENDOR: &[u8] = b"opentab-testkit";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ExtraCodebook {
    pub entries: u32,
    pub length: u8,
    pub dimensions: u16,
    pub lookup_type: u8,
    pub value_bits: u8,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FilledComments {
    pub count: u32,
    pub length: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct VorbisSpec {
    pub rate: u32,
    pub channels: u8,
    pub blocksize_exponent: u8,
    pub packets: usize,
    pub bin: usize,
    pub level_exponent: u8,
    pub floor_y: u8,
    pub serial: u32,
    pub extra_codebook: Option<ExtraCodebook>,
    pub declared_vendor_bytes: Option<u32>,
    pub declared_comments: Option<u32>,
    pub declared_comment_bytes: Option<u32>,
    pub filled_comments: Option<FilledComments>,
    pub truncate_to: Option<usize>,
}

impl VorbisSpec {
    pub const fn tone(rate: u32, packets: usize, bin: usize) -> VorbisSpec {
        VorbisSpec {
            rate,
            channels: 1,
            blocksize_exponent: 8,
            packets,
            bin,
            level_exponent: 0,
            floor_y: 63,
            serial: 0x0E7A_B5F2,
            extra_codebook: None,
            declared_vendor_bytes: None,
            declared_comments: None,
            declared_comment_bytes: None,
            filled_comments: None,
            truncate_to: None,
        }
    }

    fn half_block(&self) -> usize {
        1usize << (self.blocksize_exponent - 1)
    }
}

struct Bits {
    bytes: Vec<u8>,
    used: usize,
}

impl Bits {
    fn new() -> Bits {
        Bits {
            bytes: Vec::new(),
            used: 0,
        }
    }

    fn put(&mut self, value: u64, bits: u32) {
        for index in 0..bits {
            if self.used.is_multiple_of(8) {
                self.bytes.push(0);
            }
            let bit = ((value >> index) & 1) as u8;
            if let Some(last) = self.bytes.last_mut() {
                *last |= bit << (self.used % 8);
            }
            self.used += 1;
        }
    }

    fn flag(&mut self, value: bool) {
        self.put(u64::from(value), 1);
    }

    fn finish(self) -> Vec<u8> {
        self.bytes
    }
}

fn ilog(value: u64) -> u32 {
    64 - value.leading_zeros()
}

fn header_start(kind: u8) -> Vec<u8> {
    let mut out = vec![kind];
    out.extend_from_slice(VORBIS);
    out
}

fn ident(spec: &VorbisSpec) -> Vec<u8> {
    let mut out = header_start(IDENT_TYPE);
    out.extend_from_slice(&0u32.to_le_bytes());
    out.push(spec.channels);
    out.extend_from_slice(&spec.rate.to_le_bytes());
    out.extend_from_slice(&0i32.to_le_bytes());
    out.extend_from_slice(&0i32.to_le_bytes());
    out.extend_from_slice(&0i32.to_le_bytes());
    out.push(spec.blocksize_exponent | (spec.blocksize_exponent << 4));
    out.push(1);
    out
}

fn comment(spec: &VorbisSpec) -> Vec<u8> {
    let mut out = header_start(COMMENT_TYPE);
    let vendor = spec.declared_vendor_bytes.unwrap_or(VENDOR.len() as u32);
    out.extend_from_slice(&vendor.to_le_bytes());
    out.extend_from_slice(VENDOR);
    if let Some(filled) = spec.filled_comments {
        out.extend_from_slice(&filled.count.to_le_bytes());
        for _ in 0..filled.count {
            out.extend_from_slice(&filled.length.to_le_bytes());
            let body: Vec<u8> = (0..filled.length)
                .map(|index| if index == 1 { b'=' } else { b'x' })
                .collect();
            out.extend_from_slice(&body);
        }
        out.push(1);
        return out;
    }
    match spec.declared_comment_bytes {
        Some(length) => {
            out.extend_from_slice(&spec.declared_comments.unwrap_or(1).to_le_bytes());
            out.extend_from_slice(&length.to_le_bytes());
            out.extend_from_slice(b"A=B");
        }
        None => out.extend_from_slice(&spec.declared_comments.unwrap_or(0).to_le_bytes()),
    }
    out.push(1);
    out
}

fn two_entry_codebook(bits: &mut Bits, level_exponent: Option<u8>) {
    bits.put(CODEBOOK_SYNC, 24);
    bits.put(1, 16);
    bits.put(2, 24);
    bits.flag(false);
    bits.flag(false);
    bits.put(0, 5);
    bits.put(0, 5);
    match level_exponent {
        None => bits.put(0, 4),
        Some(exponent) => {
            bits.put(1, 4);
            bits.put(0, 32);
            bits.put(1 | ((FLOAT_BIAS + u64::from(exponent)) << 21), 32);
            bits.put(0, 4);
            bits.flag(false);
            bits.put(0, 1);
            bits.put(1, 1);
        }
    }
}

fn extra_codebook(bits: &mut Bits, book: &ExtraCodebook) {
    bits.put(CODEBOOK_SYNC, 24);
    bits.put(u64::from(book.dimensions), 16);
    bits.put(u64::from(book.entries), 24);
    bits.flag(true);
    bits.put(u64::from(book.length.saturating_sub(1)), 5);
    bits.put(u64::from(book.entries), ilog(u64::from(book.entries)));
    bits.put(u64::from(book.lookup_type), 4);
    if book.lookup_type > 0 {
        bits.put(0, 32);
        bits.put(1 | (FLOAT_BIAS << 21), 32);
        bits.put(u64::from(book.value_bits.saturating_sub(1)), 4);
        bits.flag(false);
    }
}

fn setup(spec: &VorbisSpec) -> Vec<u8> {
    let mut bits = Bits::new();
    for byte in header_start(SETUP_TYPE) {
        bits.put(u64::from(byte), 8);
    }
    let books = if spec.extra_codebook.is_some() { 3 } else { 2 };
    bits.put(books - 1, 8);
    two_entry_codebook(&mut bits, None);
    two_entry_codebook(&mut bits, Some(spec.level_exponent));
    if let Some(book) = &spec.extra_codebook {
        extra_codebook(&mut bits, book);
    }
    bits.put(0, 6);
    bits.put(0, 16);
    bits.put(0, 6);
    bits.put(FLOOR_ONE, 16);
    bits.put(0, 5);
    bits.put(FLOOR_MULTIPLIER - 1, 2);
    bits.put(u64::from(spec.blocksize_exponent - 1), 4);
    bits.put(0, 6);
    bits.put(RESIDUE_ONE, 16);
    bits.put(0, 24);
    let half = spec.half_block() as u64;
    bits.put(half, 24);
    bits.put(half - 1, 24);
    bits.put(0, 6);
    bits.put(0, 8);
    bits.put(1, 3);
    bits.flag(false);
    bits.put(1, 8);
    bits.put(0, 6);
    bits.put(0, 16);
    bits.flag(false);
    bits.flag(false);
    bits.put(0, 2);
    bits.put(0, 8);
    bits.put(0, 8);
    bits.put(0, 8);
    bits.put(0, 6);
    bits.flag(false);
    bits.put(0, 16);
    bits.put(0, 16);
    bits.put(0, 8);
    bits.flag(true);
    bits.finish()
}

fn audio_packet(spec: &VorbisSpec) -> Vec<u8> {
    let mut bits = Bits::new();
    bits.flag(false);
    for _ in 0..spec.channels {
        bits.flag(true);
        bits.put(u64::from(spec.floor_y), FLOOR_Y_BITS);
        bits.put(u64::from(spec.floor_y), FLOOR_Y_BITS);
    }
    for _ in 0..spec.channels {
        bits.put(0, 1);
    }
    for _ in 0..spec.channels {
        for index in 0..spec.half_block() {
            bits.put(u64::from(index == spec.bin), 1);
        }
    }
    bits.finish()
}

fn crc(bytes: &[u8]) -> u32 {
    let mut crc = 0u32;
    for byte in bytes {
        crc ^= u32::from(*byte) << 24;
        for _ in 0..8 {
            crc = if crc & 0x8000_0000 != 0 {
                (crc << 1) ^ CRC_POLY
            } else {
                crc << 1
            };
        }
    }
    crc
}

const MAX_SEGMENTS: usize = 255;
const CONTINUED: u8 = 0x01;
const NO_GRANULE: u64 = u64::MAX;

struct Segment {
    length: u8,
    ends_packet: bool,
}

struct Pager {
    serial: u32,
    sequence: u32,
}

impl Pager {
    fn page(&mut self, kind: u8, granule: u64, lacing: &[Segment], body: &[u8]) -> Vec<u8> {
        let mut out = Vec::new();
        out.extend_from_slice(b"OggS");
        out.push(0);
        out.push(kind);
        out.extend_from_slice(&granule.to_le_bytes());
        out.extend_from_slice(&self.serial.to_le_bytes());
        out.extend_from_slice(&self.sequence.to_le_bytes());
        out.extend_from_slice(&0u32.to_le_bytes());
        out.push(lacing.len() as u8);
        out.extend(lacing.iter().map(|segment| segment.length));
        out.extend_from_slice(body);
        let checksum = crc(&out);
        out[22..26].copy_from_slice(&checksum.to_le_bytes());
        self.sequence += 1;
        out
    }

    fn emit(&mut self, kind: u8, granule: u64, packets: &[Vec<u8>]) -> Vec<u8> {
        let mut segments = Vec::new();
        let mut body = Vec::new();
        for packet in packets {
            let mut remaining = packet.len();
            while remaining >= SEGMENT_BYTES {
                segments.push(Segment {
                    length: SEGMENT_BYTES as u8,
                    ends_packet: false,
                });
                remaining -= SEGMENT_BYTES;
            }
            segments.push(Segment {
                length: remaining as u8,
                ends_packet: true,
            });
            body.extend_from_slice(packet);
        }
        let chunks: Vec<&[Segment]> = segments.chunks(MAX_SEGMENTS).collect();
        let mut out = Vec::new();
        let mut offset = 0usize;
        let mut continued = false;
        for (index, chunk) in chunks.iter().enumerate() {
            let mut flags = if continued { CONTINUED } else { 0 };
            if index == 0 {
                flags |= kind & BEGIN_OF_STREAM;
            }
            if index + 1 == chunks.len() {
                flags |= kind & END_OF_STREAM;
            }
            let bytes: usize = chunk
                .iter()
                .map(|segment| usize::from(segment.length))
                .sum();
            let ends = chunk.iter().any(|segment| segment.ends_packet);
            let page_granule = if ends { granule } else { NO_GRANULE };
            let page = self.page(flags, page_granule, chunk, &body[offset..offset + bytes]);
            out.extend_from_slice(&page);
            offset += bytes;
            continued = chunk.last().is_some_and(|segment| !segment.ends_packet);
        }
        out
    }
}

pub fn write_vorbis(spec: &VorbisSpec) -> Vec<u8> {
    let mut out = Vec::new();
    let mut pager = Pager {
        serial: spec.serial,
        sequence: 0,
    };
    out.extend_from_slice(&pager.emit(BEGIN_OF_STREAM, 0, &[ident(spec)]));
    out.extend_from_slice(&pager.emit(0, 0, &[comment(spec), setup(spec)]));
    let packet = audio_packet(spec);
    let half = spec.half_block() as u64;
    let mut written = 0usize;
    let chunks = spec.packets.div_ceil(PACKETS_PER_PAGE).max(1);
    for chunk in 0..chunks {
        let count = (spec.packets - written).min(PACKETS_PER_PAGE);
        written += count;
        let last = chunk + 1 == chunks;
        let kind = if last { END_OF_STREAM } else { 0 };
        let granule = (written as u64).saturating_sub(1) * half;
        let packets = vec![packet.clone(); count];
        out.extend_from_slice(&pager.emit(kind, granule, &packets));
    }
    if let Some(length) = spec.truncate_to {
        out.truncate(length);
    }
    out
}

const PAGE_HEADER_BYTES: usize = 27;
const CRC_OFFSET: usize = 22;
const SEGMENT_COUNT_OFFSET: usize = 26;

pub fn reseal_ogg(bytes: &mut [u8]) {
    let mut at = 0usize;
    while at + PAGE_HEADER_BYTES <= bytes.len() && &bytes[at..at + 4] == b"OggS" {
        let segments = usize::from(bytes[at + SEGMENT_COUNT_OFFSET]);
        let table_end = at + PAGE_HEADER_BYTES + segments;
        if table_end > bytes.len() {
            return;
        }
        let body: usize = bytes[at + PAGE_HEADER_BYTES..table_end]
            .iter()
            .map(|segment| usize::from(*segment))
            .sum();
        let end = table_end + body;
        if end > bytes.len() {
            return;
        }
        bytes[at + CRC_OFFSET..at + CRC_OFFSET + 4].fill(0);
        let checksum = crc(&bytes[at..end]);
        bytes[at + CRC_OFFSET..at + CRC_OFFSET + 4].copy_from_slice(&checksum.to_le_bytes());
        at = end;
    }
}
