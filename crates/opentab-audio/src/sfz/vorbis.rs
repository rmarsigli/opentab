use crate::sfz::limits::SfzLimits;
use crate::{BankError, UnsafeBank};

const HEADER_BYTES: usize = 7;
const COMMENT_FIXED_BYTES: u64 = 8;
const CODEBOOK_SYNC: u32 = 0x56_4342;
const IDENT_CHANNELS: usize = 11;
const IDENT_RATE: usize = 12;
const LENGTH_FIELD_BYTES: u64 = 4;
const BOOK_BYTES: u64 = 4096;
const LENGTH_BYTES: u64 = 1;
const DESC_CAPACITY_BYTES: u64 = 4;
const NODE_BYTES: u64 = 48;
const DESC_NODE_BYTES: u64 = 24;
const VALUE_BYTES: u64 = 4;
const VECTOR_BYTES: u64 = 4;

fn damaged() -> BankError {
    BankError::Invalid {
        reason: "an Ogg Vorbis sample has a damaged header".to_owned(),
    }
}

fn refuse(reason: UnsafeBank) -> BankError {
    BankError::Unsafe { reason }
}

struct BitCursor<'a> {
    bytes: &'a [u8],
    position: u64,
}

impl BitCursor<'_> {
    fn remaining(&self) -> u64 {
        (self.bytes.len() as u64 * 8).saturating_sub(self.position)
    }

    fn read(&mut self, bits: u32) -> Result<u32, BankError> {
        if u64::from(bits) > self.remaining() {
            return Err(damaged());
        }
        let mut value = 0u32;
        for index in 0..bits {
            let byte = self.bytes[(self.position / 8) as usize];
            let bit = (byte >> (self.position % 8)) & 1;
            value |= u32::from(bit) << index;
            self.position += 1;
        }
        Ok(value)
    }

    fn skip(&mut self, bits: u64) -> Result<(), BankError> {
        if bits > self.remaining() {
            return Err(damaged());
        }
        self.position += bits;
        Ok(())
    }
}

fn ilog(value: u64) -> u32 {
    64 - value.leading_zeros()
}

fn power_fits(base: u64, dimensions: u16, entries: u64) -> bool {
    let mut power: u64 = 1;
    for _ in 0..dimensions {
        power = power.saturating_mul(base);
        if power > entries {
            return false;
        }
    }
    true
}

fn lookup1_values(entries: u32, dimensions: u16) -> u64 {
    if dimensions == 0 {
        return u64::from(u32::MAX);
    }
    let entries = u64::from(entries);
    let (mut low, mut high) = (0u64, entries + 1);
    while high - low > 1 {
        let middle = low + (high - low) / 2;
        if power_fits(middle, dimensions, entries) {
            low = middle;
        } else {
            high = middle;
        }
    }
    if entries == 0 { 0 } else { low.max(1) }
}

pub(crate) struct VorbisShape {
    pub(crate) channels: u32,
    pub(crate) rate: u32,
}

pub(crate) fn check_ident(packet: &[u8]) -> Result<VorbisShape, BankError> {
    if packet.len() < IDENT_RATE + 4 {
        return Err(damaged());
    }
    let rate = u32::from_le_bytes([
        packet[IDENT_RATE],
        packet[IDENT_RATE + 1],
        packet[IDENT_RATE + 2],
        packet[IDENT_RATE + 3],
    ]);
    Ok(VorbisShape {
        channels: u32::from(packet[IDENT_CHANNELS]),
        rate,
    })
}

pub(crate) fn check_comment(packet: &[u8]) -> Result<(), BankError> {
    let mut at = HEADER_BYTES;
    let read_length = |at: &mut usize| -> Result<u64, BankError> {
        let bytes = packet.get(*at..*at + 4).ok_or_else(damaged)?;
        *at += 4;
        Ok(u64::from(u32::from_le_bytes([
            bytes[0], bytes[1], bytes[2], bytes[3],
        ])))
    };
    let total = packet.len() as u64;
    let vendor = read_length(&mut at)?;
    if vendor > total.saturating_sub(at as u64 + LENGTH_FIELD_BYTES) {
        return Err(refuse(UnsafeBank::VorbisComments));
    }
    at += vendor as usize;
    let comments = read_length(&mut at)?;
    if comments.saturating_mul(LENGTH_FIELD_BYTES) > total.saturating_sub(at as u64) {
        return Err(refuse(UnsafeBank::VorbisComments));
    }
    for _ in 0..comments {
        let length = read_length(&mut at)?;
        if length > total.saturating_sub(at as u64) {
            return Err(refuse(UnsafeBank::VorbisComments));
        }
        at += length as usize;
    }
    if total < COMMENT_FIXED_BYTES {
        return Err(damaged());
    }
    Ok(())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct VorbisSetupCost {
    pub entries: u64,
    pub bytes: u64,
}

impl VorbisSetupCost {
    fn add(&mut self, bytes: u64, limits: &SfzLimits) -> Result<(), BankError> {
        self.bytes = self.bytes.saturating_add(bytes);
        if self.bytes > limits.vorbis_setup_bytes {
            return Err(refuse(UnsafeBank::VorbisTables));
        }
        Ok(())
    }
}

fn codeword_lengths(
    cursor: &mut BitCursor<'_>,
    entries: u32,
    ordered: bool,
) -> Result<u64, BankError> {
    let mut total: u64 = 0;
    if ordered {
        let mut length = u64::from(cursor.read(5)?) + 1;
        let mut current: u32 = 0;
        while current < entries {
            let number = cursor.read(ilog(u64::from(entries - current)))?;
            current = current.saturating_add(number);
            if current > entries {
                return Err(damaged());
            }
            total = total.saturating_add(u64::from(number).saturating_mul(length));
            length += 1;
        }
    } else {
        let sparse = cursor.read(1)? == 1;
        for _ in 0..entries {
            if !sparse || cursor.read(1)? == 1 {
                total = total.saturating_add(u64::from(cursor.read(5)?) + 1);
            }
        }
    }
    Ok(total)
}

pub(crate) fn check_setup(packet: &[u8], limits: &SfzLimits) -> Result<VorbisSetupCost, BankError> {
    let body = packet.get(HEADER_BYTES..).ok_or_else(damaged)?;
    let mut cursor = BitCursor {
        bytes: body,
        position: 0,
    };
    let books = cursor.read(8)? + 1;
    let mut cost = VorbisSetupCost::default();
    for _ in 0..books {
        if cursor.read(24)? != CODEBOOK_SYNC {
            return Err(damaged());
        }
        let dimensions = cursor.read(16)? as u16;
        let entries = cursor.read(24)?;
        let ordered = cursor.read(1)? == 1;
        cost.entries = cost.entries.saturating_add(u64::from(entries));
        if cost.entries > limits.vorbis_entries {
            return Err(refuse(UnsafeBank::VorbisEntries {
                limit: limits.vorbis_entries,
            }));
        }
        cost.add(
            BOOK_BYTES + u64::from(entries) * (LENGTH_BYTES + DESC_CAPACITY_BYTES),
            limits,
        )?;
        let tree_nodes = codeword_lengths(&mut cursor, entries, ordered)?.saturating_add(1);
        cost.add(
            tree_nodes.saturating_mul(NODE_BYTES + DESC_NODE_BYTES),
            limits,
        )?;
        let lookup = cursor.read(4)?;
        if lookup == 0 {
            continue;
        }
        if lookup > 2 {
            return Err(damaged());
        }
        cursor.skip(64)?;
        let value_bits = u64::from(cursor.read(4)?) + 1;
        cursor.read(1)?;
        let values = if lookup == 1 {
            lookup1_values(entries, dimensions)
        } else {
            u64::from(entries) * u64::from(dimensions)
        };
        let vectors = u64::from(entries).saturating_mul(u64::from(dimensions));
        cost.add(
            values
                .saturating_mul(VALUE_BYTES)
                .saturating_add(vectors.saturating_mul(VECTOR_BYTES)),
            limits,
        )?;
        cursor.skip(values.saturating_mul(value_bits))?;
    }
    Ok(cost)
}
