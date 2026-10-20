use std::collections::HashMap;
use std::ops::Range;

use super::riff::{Layout, Table};
use super::source::Source;
use crate::{BankError, Sf2Limits, UnsafeBank};

const PRESET_BAG_OFFSET: usize = 24;
const PRESET_PROGRAM_OFFSET: usize = 20;
const PRESET_BANK_OFFSET: usize = 22;
const INSTRUMENT_BAG_OFFSET: usize = 20;
const BAG_MODULATOR_OFFSET: usize = 2;

const INSTRUMENT_GENERATOR: u16 = 41;
const KEY_RANGE_GENERATOR: u16 = 43;
const VELOCITY_RANGE_GENERATOR: u16 = 44;
const SAMPLE_GENERATOR: u16 = 53;
const FULL_RANGE: u16 = 0x7F00;

const GROWTH: u64 = 2;
const FIXED_COST: u64 = 256 << 10;
const PRESET_COST: u64 = 768;
const INSTRUMENT_COST: u64 = 192;
const ZONE_COST: u64 = 64;
const GENERATOR_COST: u64 = 8;
const REGION_COST: u64 = 256;
const SAMPLE_HEADER_COST: u64 = 160;
const INFO_COPIES: u64 = 2;

pub(crate) const KEYS: usize = 128;
const VELOCITIES: usize = 128;
const FIRST_VELOCITY: u8 = 1;
const TOP: u8 = 127;
const GRID_SIDE: usize = KEYS + 1;
const PERCUSSION_BANK: i32 = 128;
const PATCH_BITS: u32 = 16;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub struct BankShape {
    pub loader_bytes: u64,
    pub region_pairs: u64,
    pub max_preset_pairs: u64,
    pub layer_bound: u32,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct LayerTable {
    lookup: HashMap<i32, usize>,
    default_preset: usize,
    layers: Vec<Option<[u32; KEYS]>>,
}

impl LayerTable {
    pub(crate) fn resolve(&self, bank: i32, patch: i32) -> usize {
        let id = (bank << PATCH_BITS) | patch;
        if let Some(found) = self.lookup.get(&id) {
            return *found;
        }
        let fallback = if bank < PERCUSSION_BANK {
            patch
        } else {
            PERCUSSION_BANK << PATCH_BITS
        };
        self.lookup
            .get(&fallback)
            .copied()
            .unwrap_or(self.default_preset)
    }

    pub(crate) fn layers(&self, preset: usize, key: u8) -> u32 {
        self.layers
            .get(preset)
            .and_then(|table| table.as_ref())
            .and_then(|table| table.get(usize::from(key)).copied())
            .unwrap_or(0)
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct Analysis {
    pub(crate) shape: BankShape,
    pub(crate) table: LayerTable,
}

struct Records<'a> {
    bytes: &'a [u8],
    count: usize,
    width: usize,
}

impl<'a> Records<'a> {
    fn new<S: Source + ?Sized>(bytes: &'a S, table: &Table) -> Result<Records<'a>, BankError> {
        Ok(Records {
            bytes: bytes
                .slice(table.range.clone())
                .ok_or(refuse(UnsafeBank::ChunkOverflow))?,
            count: table.count,
            width: table.range.len() / table.count.max(1),
        })
    }

    fn word(&self, record: usize, offset: usize) -> u16 {
        let at = record * self.width + offset;
        u16::from_le_bytes([self.bytes[at], self.bytes[at + 1]])
    }

    fn column(&self, offset: usize) -> Vec<usize> {
        (0..self.count)
            .map(|record| usize::from(self.word(record, offset)))
            .collect()
    }
}

fn refuse(reason: UnsafeBank) -> BankError {
    BankError::Unsafe { reason }
}

fn ordered(indices: &[usize], limit: usize) -> bool {
    indices.windows(2).all(|pair| pair[0] <= pair[1])
        && indices.last().is_none_or(|last| *last <= limit)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Span {
    low: u8,
    high: u8,
}

impl Span {
    fn of(value: u16) -> Span {
        let [low, high] = value.to_le_bytes();
        Span { low, high }
    }

    fn meet(self, other: Span, floor: u8) -> Option<Span> {
        let low = self.low.max(other.low).max(floor);
        let high = self.high.min(other.high).min(TOP);
        (low <= high).then_some(Span { low, high })
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
struct ZoneFacts {
    keys: Option<u16>,
    velocities: Option<u16>,
    target: Option<u16>,
    last: Option<u16>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Region {
    keys: Span,
    velocities: Span,
    target: u16,
}

fn zone_facts(bags: &[usize], generators: &Records<'_>, terminal: u16) -> Vec<ZoneFacts> {
    bags.windows(2)
        .map(|pair| {
            let mut facts = ZoneFacts::default();
            for record in pair[0]..pair[1] {
                let operator = generators.word(record, 0);
                let amount = generators.word(record, 2);
                match operator {
                    KEY_RANGE_GENERATOR => facts.keys = Some(amount),
                    VELOCITY_RANGE_GENERATOR => facts.velocities = Some(amount),
                    other if other == terminal => facts.target = Some(amount),
                    _ => {}
                }
                facts.last = Some(operator);
            }
            facts
        })
        .collect()
}

fn regions(zones: &[ZoneFacts], terminal: u16) -> Vec<Region> {
    let Some(first) = zones.first() else {
        return Vec::new();
    };
    let (global, locals) = if first.last != Some(terminal) {
        (*first, &zones[1..])
    } else {
        (ZoneFacts::default(), zones)
    };
    locals
        .iter()
        .map(|local| Region {
            keys: Span::of(local.keys.or(global.keys).unwrap_or(FULL_RANGE)),
            velocities: Span::of(local.velocities.or(global.velocities).unwrap_or(FULL_RANGE)),
            target: local.target.or(global.target).unwrap_or(0),
        })
        .collect()
}

fn spans(owners: &[usize], zones: &[ZoneFacts]) -> Vec<Range<usize>> {
    owners
        .windows(2)
        .map(|pair| pair[0]..pair[1].max(pair[0]).min(zones.len()))
        .collect()
}

fn layer_grid(preset: &[Region], instruments: &[Vec<Region>], grid: &mut [i64]) -> [u32; KEYS] {
    grid.fill(0);
    for outer in preset {
        let Some(inner) = instruments.get(usize::from(outer.target)) else {
            continue;
        };
        for region in inner {
            let (Some(keys), Some(velocities)) = (
                outer.keys.meet(region.keys, 0),
                outer.velocities.meet(region.velocities, FIRST_VELOCITY),
            ) else {
                continue;
            };
            let (k0, k1) = (usize::from(keys.low), usize::from(keys.high) + 1);
            let (v0, v1) = (
                usize::from(velocities.low),
                usize::from(velocities.high) + 1,
            );
            grid[k0 * GRID_SIDE + v0] += 1;
            grid[k0 * GRID_SIDE + v1] -= 1;
            grid[k1 * GRID_SIDE + v0] -= 1;
            grid[k1 * GRID_SIDE + v1] += 1;
        }
    }
    for key in 0..GRID_SIDE {
        for velocity in 1..GRID_SIDE {
            grid[key * GRID_SIDE + velocity] += grid[key * GRID_SIDE + velocity - 1];
        }
    }
    for key in 1..GRID_SIDE {
        for velocity in 0..GRID_SIDE {
            grid[key * GRID_SIDE + velocity] += grid[(key - 1) * GRID_SIDE + velocity];
        }
    }
    let mut most = [0u32; KEYS];
    for (key, slot) in most.iter_mut().enumerate() {
        let row = &grid[key * GRID_SIDE..key * GRID_SIDE + VELOCITIES];
        let peak = row.iter().copied().max().unwrap_or(0);
        *slot = u32::try_from(peak.max(0)).unwrap_or(u32::MAX);
    }
    most
}

fn sum(parts: &[u64]) -> Option<u64> {
    parts
        .iter()
        .try_fold(0u64, |total, part| total.checked_add(*part))
}

fn estimate(layout: &Layout) -> Option<u64> {
    let [phdr, pbag, pmod, pgen, inst, ibag, imod, igen, shdr] = &layout.tables;
    let count = |table: &Table| widen(table.count);
    let bags = count(pbag).checked_add(count(ibag))?;
    let generators = count(pgen).checked_add(count(igen))?;
    let structures = sum(&[
        count(phdr).checked_mul(PRESET_COST)?,
        count(inst).checked_mul(INSTRUMENT_COST)?,
        bags.checked_mul(ZONE_COST + REGION_COST)?,
        generators.checked_mul(GENERATOR_COST)?,
        count(shdr).checked_mul(SAMPLE_HEADER_COST)?,
    ])?;
    sum(&[
        FIXED_COST,
        layout.sample_bytes,
        layout.extra_sample_bytes,
        layout.info_bytes.checked_mul(INFO_COPIES)?,
        widen(pmod.range.len()),
        widen(imod.range.len()),
        structures.checked_mul(GROWTH)?,
    ])
}

fn widen(value: usize) -> u64 {
    u64::try_from(value).unwrap_or(u64::MAX)
}

pub(crate) fn analyse<S: Source + ?Sized>(
    bytes: &S,
    layout: &Layout,
    limits: &Sf2Limits,
) -> Result<Analysis, BankError> {
    let loader = estimate(layout).ok_or(refuse(UnsafeBank::LoaderBudget))?;
    let allowance = limits
        .loader_allowance(layout.file_bytes)
        .ok_or(refuse(UnsafeBank::LoaderBudget))?;
    if loader > allowance {
        return Err(refuse(UnsafeBank::LoaderBudget));
    }
    let [phdr, pbag, pmod, pgen, inst, ibag, imod, igen, _] = &layout.tables;
    let preset_modulators = pmod.count;
    let instrument_modulators = imod.count;
    let phdr = Records::new(bytes, phdr)?;
    let pbag = Records::new(bytes, pbag)?;
    let pgen = Records::new(bytes, pgen)?;
    let inst = Records::new(bytes, inst)?;
    let ibag = Records::new(bytes, ibag)?;
    let igen = Records::new(bytes, igen)?;

    let preset_owners = phdr.column(PRESET_BAG_OFFSET);
    let instrument_owners = inst.column(INSTRUMENT_BAG_OFFSET);
    if !ordered(&preset_owners, pbag.count - 1) || !ordered(&instrument_owners, ibag.count - 1) {
        return Err(refuse(UnsafeBank::ZoneOrder));
    }
    let preset_bags = pbag.column(0);
    let instrument_bags = ibag.column(0);
    if !ordered(&preset_bags, pgen.count - 1) || !ordered(&instrument_bags, igen.count - 1) {
        return Err(refuse(UnsafeBank::GeneratorOrder));
    }
    if !ordered(
        &pbag.column(BAG_MODULATOR_OFFSET),
        preset_modulators.saturating_sub(1),
    ) || !ordered(
        &ibag.column(BAG_MODULATOR_OFFSET),
        instrument_modulators.saturating_sub(1),
    ) {
        return Err(refuse(UnsafeBank::ModulatorOrder));
    }
    let instrument_zones = zone_facts(&instrument_bags, &igen, SAMPLE_GENERATOR);
    let instruments: Vec<Vec<Region>> = spans(&instrument_owners, &instrument_zones)
        .into_iter()
        .map(|span| regions(&instrument_zones[span], SAMPLE_GENERATOR))
        .collect();
    let preset_zones = zone_facts(&preset_bags, &pgen, INSTRUMENT_GENERATOR);
    let presets: Vec<Vec<Region>> = spans(&preset_owners, &preset_zones)
        .into_iter()
        .map(|span| regions(&preset_zones[span], INSTRUMENT_GENERATOR))
        .collect();

    let mut shape = BankShape {
        loader_bytes: loader,
        ..BankShape::default()
    };
    let mut pairs_of = Vec::with_capacity(presets.len());
    for preset in &presets {
        let pairs: u64 = preset
            .iter()
            .map(|region| {
                instruments
                    .get(usize::from(region.target))
                    .map_or(0, |inner| widen(inner.len()))
            })
            .sum();
        if pairs > limits.pairs_per_preset {
            return Err(refuse(UnsafeBank::RegionPairs));
        }
        shape.region_pairs += pairs;
        shape.max_preset_pairs = shape.max_preset_pairs.max(pairs);
        pairs_of.push(pairs);
    }
    if shape.region_pairs > limits.region_pairs {
        return Err(refuse(UnsafeBank::RegionPairs));
    }

    let watch = u64::from(limits.layers_per_note.min(limits.voices_per_note));
    let gridded = pairs_of.iter().filter(|pairs| **pairs > watch).count();
    if widen(gridded) > limits.grid_presets {
        return Err(refuse(UnsafeBank::LayeredPresets));
    }
    let mut grid: Vec<i64> = Vec::new();
    let mut table = LayerTable::default();
    for (preset, pairs) in presets.iter().zip(&pairs_of) {
        if *pairs <= watch {
            shape.layer_bound = shape
                .layer_bound
                .max(u32::try_from(*pairs).unwrap_or(u32::MAX));
            table.layers.push(None);
            continue;
        }
        if grid.is_empty() {
            grid = vec![0; GRID_SIDE * GRID_SIDE];
        }
        let most = layer_grid(preset, &instruments, &mut grid);
        let peak = most.iter().copied().max().unwrap_or(0);
        if peak > limits.layers_per_note {
            return Err(refuse(UnsafeBank::Layers));
        }
        shape.layer_bound = shape.layer_bound.max(peak);
        table.layers.push(Some(most));
    }

    let mut lowest = i32::MAX;
    for index in 0..presets.len() {
        let id = (i32::from(phdr.word(index, PRESET_BANK_OFFSET)) << PATCH_BITS)
            | i32::from(phdr.word(index, PRESET_PROGRAM_OFFSET));
        table.lookup.insert(id, index);
        if id < lowest {
            lowest = id;
            table.default_preset = index;
        }
    }
    Ok(Analysis { shape, table })
}
