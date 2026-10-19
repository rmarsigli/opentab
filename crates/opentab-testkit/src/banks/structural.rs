use proptest::prelude::*;

const HEADER: usize = 8;
const FORM: usize = 4;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PdtaTable {
    Presets,
    PresetBags,
    PresetModulators,
    PresetGenerators,
    Instruments,
    InstrumentBags,
    InstrumentModulators,
    InstrumentGenerators,
    SampleHeaders,
}

impl PdtaTable {
    pub const ALL: [PdtaTable; 9] = [
        PdtaTable::Presets,
        PdtaTable::PresetBags,
        PdtaTable::PresetModulators,
        PdtaTable::PresetGenerators,
        PdtaTable::Instruments,
        PdtaTable::InstrumentBags,
        PdtaTable::InstrumentModulators,
        PdtaTable::InstrumentGenerators,
        PdtaTable::SampleHeaders,
    ];

    pub fn id(self) -> &'static [u8; 4] {
        match self {
            PdtaTable::Presets => b"phdr",
            PdtaTable::PresetBags => b"pbag",
            PdtaTable::PresetModulators => b"pmod",
            PdtaTable::PresetGenerators => b"pgen",
            PdtaTable::Instruments => b"inst",
            PdtaTable::InstrumentBags => b"ibag",
            PdtaTable::InstrumentModulators => b"imod",
            PdtaTable::InstrumentGenerators => b"igen",
            PdtaTable::SampleHeaders => b"shdr",
        }
    }

    pub fn width(self) -> usize {
        match self {
            PdtaTable::Presets => 38,
            PdtaTable::Instruments => 22,
            PdtaTable::PresetModulators | PdtaTable::InstrumentModulators => 10,
            PdtaTable::SampleHeaders => 46,
            PdtaTable::PresetBags
            | PdtaTable::PresetGenerators
            | PdtaTable::InstrumentBags
            | PdtaTable::InstrumentGenerators => 4,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PdtaField {
    PresetBag,
    PresetGenerator,
    PresetModulator,
    InstrumentBag,
    InstrumentGenerator,
    InstrumentModulator,
    GeneratorOperator,
    GeneratorAmount,
}

impl PdtaField {
    fn place(self) -> FieldPlace {
        let (table, offset) = match self {
            PdtaField::PresetBag => (PdtaTable::Presets, 24),
            PdtaField::PresetGenerator => (PdtaTable::PresetBags, 0),
            PdtaField::PresetModulator => (PdtaTable::PresetBags, 2),
            PdtaField::InstrumentBag => (PdtaTable::Instruments, 20),
            PdtaField::InstrumentGenerator => (PdtaTable::InstrumentBags, 0),
            PdtaField::InstrumentModulator => (PdtaTable::InstrumentBags, 2),
            PdtaField::GeneratorOperator => (PdtaTable::InstrumentGenerators, 0),
            PdtaField::GeneratorAmount => (PdtaTable::InstrumentGenerators, 2),
        };
        FieldPlace { table, offset }
    }
}

struct FieldPlace {
    table: PdtaTable,
    offset: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum FieldValue {
    Exact(u16),
    Shift(i16),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum StructuralMutation {
    Field {
        field: PdtaField,
        record: usize,
        value: FieldValue,
    },
    Records {
        table: PdtaTable,
        delta: i8,
    },
}

pub fn arb_structural_mutation() -> impl Strategy<Value = StructuralMutation> {
    let field = prop::sample::select(vec![
        PdtaField::PresetBag,
        PdtaField::PresetGenerator,
        PdtaField::PresetModulator,
        PdtaField::InstrumentBag,
        PdtaField::InstrumentGenerator,
        PdtaField::InstrumentModulator,
        PdtaField::GeneratorOperator,
        PdtaField::GeneratorAmount,
    ]);
    let value = prop_oneof![
        Just(FieldValue::Exact(0)),
        Just(FieldValue::Exact(u16::MAX)),
        Just(FieldValue::Exact(0x7FFF)),
        (-3i16..=3).prop_map(FieldValue::Shift),
        any::<u16>().prop_map(FieldValue::Exact),
    ];
    prop_oneof![
        3 => (field, 0usize..64, value)
            .prop_map(|(field, record, value)| StructuralMutation::Field { field, record, value }),
        1 => (prop::sample::select(PdtaTable::ALL.to_vec()), -3i8..=3)
            .prop_map(|(table, delta)| StructuralMutation::Records { table, delta }),
    ]
}

struct Located {
    header: usize,
    size: usize,
}

fn read_size(bytes: &[u8], header: usize) -> usize {
    let mut size = [0u8; 4];
    size.copy_from_slice(&bytes[header + 4..header + HEADER]);
    u32::from_le_bytes(size) as usize
}

fn write_size(bytes: &mut [u8], header: usize, size: usize) {
    let size = u32::try_from(size).unwrap_or(u32::MAX);
    bytes[header + 4..header + HEADER].copy_from_slice(&size.to_le_bytes());
}

fn pdta_list(bytes: &[u8]) -> Option<usize> {
    bytes
        .windows(FORM)
        .position(|window| window == b"pdta")
        .and_then(|at| at.checked_sub(HEADER))
}

fn locate(bytes: &[u8], id: &[u8; 4]) -> Option<Located> {
    let list = pdta_list(bytes)?;
    let end = (list + HEADER + read_size(bytes, list)).min(bytes.len());
    let mut at = list + HEADER + FORM;
    let mut found = None;
    while at + HEADER <= end {
        let size = read_size(bytes, at);
        if &bytes[at..at + FORM] == id {
            found = Some(Located { header: at, size });
        }
        at = at.checked_add(HEADER + size)?;
    }
    found
}

fn set_field(bytes: &mut [u8], field: PdtaField, record: usize, value: FieldValue) {
    let place = field.place();
    let Some(chunk) = locate(bytes, place.table.id()) else {
        return;
    };
    let records = chunk.size / place.table.width();
    if records == 0 {
        return;
    }
    let at = chunk.header + HEADER + (record % records) * place.table.width() + place.offset;
    if at + 2 > bytes.len() {
        return;
    }
    let current = u16::from_le_bytes([bytes[at], bytes[at + 1]]);
    let next = match value {
        FieldValue::Exact(value) => value,
        FieldValue::Shift(shift) => current.wrapping_add_signed(shift),
    };
    bytes[at..at + 2].copy_from_slice(&next.to_le_bytes());
}

fn resize(bytes: &[u8], table: PdtaTable, delta: i8) -> Vec<u8> {
    let (Some(chunk), Some(list)) = (locate(bytes, table.id()), pdta_list(bytes)) else {
        return bytes.to_vec();
    };
    let width = table.width();
    let body = chunk.header + HEADER;
    let end = body + chunk.size;
    let mut out = bytes[..end].to_vec();
    let change = usize::from(delta.unsigned_abs()) * width;
    let grown = if delta >= 0 {
        let source = end.saturating_sub(width).max(body);
        let record = bytes[source..end].to_vec();
        for _ in 0..delta {
            out.extend_from_slice(&record);
        }
        chunk.size + change
    } else {
        let removed = change.min(chunk.size);
        out.truncate(end - removed);
        chunk.size - removed
    };
    out.extend_from_slice(&bytes[end..]);
    write_size(&mut out, chunk.header, grown);
    let list_size = read_size(&out, list) - chunk.size + grown;
    write_size(&mut out, list, list_size);
    let riff = out.len() - HEADER;
    write_size(&mut out, 0, riff);
    out
}

pub fn apply_structural(bytes: &[u8], mutations: &[StructuralMutation]) -> Vec<u8> {
    let mut out = bytes.to_vec();
    for mutation in mutations {
        match *mutation {
            StructuralMutation::Field {
                field,
                record,
                value,
            } => set_field(&mut out, field, record, value),
            StructuralMutation::Records { table, delta } => out = resize(&out, table, delta),
        }
    }
    out
}
