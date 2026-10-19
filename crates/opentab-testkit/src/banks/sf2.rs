const NAME_BYTES: usize = 20;
const GUARD_POINTS: usize = 46;
const MONO_SAMPLE: u16 = 1;
const VERSION_MAJOR: u16 = 2;
const VERSION_MINOR: u16 = 4;
const SOUND_ENGINE: &str = "EMU8000";
const SOFTWARE: &str = "opentab-testkit";
const MODULATOR_BYTES: usize = 10;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Generator {
    pub operator: u16,
    pub amount: u16,
}

pub fn generator(operator: u16, amount: i16) -> Generator {
    Generator {
        operator,
        amount: u16::from_le_bytes(amount.to_le_bytes()),
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Zone {
    pub generators: Vec<Generator>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SampleDef {
    pub name: String,
    pub points: Vec<i16>,
    pub loop_start: u32,
    pub loop_end: u32,
    pub rate: u32,
    pub root_key: u8,
    pub correction: i8,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InstrumentDef {
    pub name: String,
    pub zones: Vec<Zone>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PresetDef {
    pub name: String,
    pub bank: u16,
    pub program: u16,
    pub zones: Vec<Zone>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SoundFontDef {
    pub name: String,
    pub samples: Vec<SampleDef>,
    pub instruments: Vec<InstrumentDef>,
    pub presets: Vec<PresetDef>,
}

fn chunk(id: &[u8; 4], body: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(body.len() + 9);
    out.extend_from_slice(id);
    out.extend_from_slice(&size_of_body(body.len()).to_le_bytes());
    out.extend_from_slice(body);
    if body.len() % 2 == 1 {
        out.push(0);
    }
    out
}

fn size_of_body(length: usize) -> u32 {
    u32::try_from(length).unwrap_or(u32::MAX)
}

fn list(kind: &[u8; 4], chunks: &[Vec<u8>]) -> Vec<u8> {
    let mut body = kind.to_vec();
    for part in chunks {
        body.extend_from_slice(part);
    }
    chunk(b"LIST", &body)
}

fn text(value: &str) -> Vec<u8> {
    let mut bytes: Vec<u8> = value.bytes().filter(u8::is_ascii).collect();
    bytes.push(0);
    if bytes.len() % 2 == 1 {
        bytes.push(0);
    }
    bytes
}

fn name(value: &str) -> [u8; NAME_BYTES] {
    let mut out = [0u8; NAME_BYTES];
    for (target, byte) in out
        .iter_mut()
        .zip(value.bytes().filter(u8::is_ascii).take(NAME_BYTES - 1))
    {
        *target = byte;
    }
    out
}

fn index(value: usize) -> [u8; 2] {
    u16::try_from(value).unwrap_or(u16::MAX).to_le_bytes()
}

fn word(value: usize) -> [u8; 4] {
    size_of_body(value).to_le_bytes()
}

fn info(definition: &SoundFontDef) -> Vec<u8> {
    let mut version = Vec::new();
    version.extend_from_slice(&VERSION_MAJOR.to_le_bytes());
    version.extend_from_slice(&VERSION_MINOR.to_le_bytes());
    list(
        b"INFO",
        &[
            chunk(b"ifil", &version),
            chunk(b"isng", &text(SOUND_ENGINE)),
            chunk(b"INAM", &text(&definition.name)),
            chunk(b"ISFT", &text(SOFTWARE)),
        ],
    )
}

struct SampleLayout {
    data: Vec<u8>,
    headers: Vec<u8>,
}

fn sample_layout(samples: &[SampleDef]) -> SampleLayout {
    let mut data = Vec::new();
    let mut headers = Vec::new();
    let mut offset = 0usize;
    for sample in samples {
        for point in &sample.points {
            data.extend_from_slice(&point.to_le_bytes());
        }
        data.extend(std::iter::repeat_n(0u8, GUARD_POINTS * 2));
        let start = offset;
        let end = offset + sample.points.len();
        let (loop_start, loop_end) = if sample.loop_end > sample.loop_start {
            (
                start + sample.loop_start as usize,
                start + sample.loop_end as usize,
            )
        } else {
            (start, end)
        };
        headers.extend_from_slice(&name(&sample.name));
        headers.extend_from_slice(&word(start));
        headers.extend_from_slice(&word(end));
        headers.extend_from_slice(&word(loop_start));
        headers.extend_from_slice(&word(loop_end));
        headers.extend_from_slice(&sample.rate.to_le_bytes());
        headers.push(sample.root_key);
        headers.extend_from_slice(&sample.correction.to_le_bytes());
        headers.extend_from_slice(&0u16.to_le_bytes());
        headers.extend_from_slice(&MONO_SAMPLE.to_le_bytes());
        offset = end + GUARD_POINTS;
    }
    headers.extend_from_slice(&name("EOS"));
    headers.extend(std::iter::repeat_n(0u8, 26));
    SampleLayout { data, headers }
}

struct ZoneLists {
    bags: Vec<u8>,
    generators: Vec<u8>,
    modulators: Vec<u8>,
}

fn zone_lists<'a>(zones: impl Iterator<Item = &'a Zone>) -> ZoneLists {
    let mut lists = ZoneLists {
        bags: Vec::new(),
        generators: Vec::new(),
        modulators: vec![0u8; MODULATOR_BYTES],
    };
    let mut generator_count = 0usize;
    for zone in zones {
        lists.bags.extend_from_slice(&index(generator_count));
        lists.bags.extend_from_slice(&index(0));
        for item in &zone.generators {
            lists
                .generators
                .extend_from_slice(&item.operator.to_le_bytes());
            lists
                .generators
                .extend_from_slice(&item.amount.to_le_bytes());
            generator_count += 1;
        }
    }
    lists.bags.extend_from_slice(&index(generator_count));
    lists.bags.extend_from_slice(&index(0));
    lists.generators.extend_from_slice(&[0u8; 4]);
    lists
}

fn zone_starts(counts: impl Iterator<Item = usize>) -> Vec<usize> {
    let mut starts = Vec::new();
    let mut total = 0usize;
    for count in counts {
        starts.push(total);
        total += count;
    }
    starts.push(total);
    starts
}

pub fn write_sf2(definition: &SoundFontDef) -> Vec<u8> {
    let layout = sample_layout(&definition.samples);
    let preset_lists = zone_lists(
        definition
            .presets
            .iter()
            .flat_map(|preset| preset.zones.iter()),
    );
    let preset_starts = zone_starts(definition.presets.iter().map(|preset| preset.zones.len()));
    let mut headers = Vec::new();
    for (preset, start) in definition.presets.iter().zip(&preset_starts) {
        headers.extend_from_slice(&name(&preset.name));
        headers.extend_from_slice(&preset.program.to_le_bytes());
        headers.extend_from_slice(&preset.bank.to_le_bytes());
        headers.extend_from_slice(&index(*start));
        headers.extend(std::iter::repeat_n(0u8, 12));
    }
    headers.extend_from_slice(&name("EOP"));
    headers.extend(std::iter::repeat_n(0u8, 4));
    headers.extend_from_slice(&index(preset_starts.last().copied().unwrap_or(0)));
    headers.extend(std::iter::repeat_n(0u8, 12));
    let instrument_lists = zone_lists(
        definition
            .instruments
            .iter()
            .flat_map(|instrument| instrument.zones.iter()),
    );
    let instrument_starts = zone_starts(
        definition
            .instruments
            .iter()
            .map(|instrument| instrument.zones.len()),
    );
    let mut instruments = Vec::new();
    for (instrument, start) in definition.instruments.iter().zip(&instrument_starts) {
        instruments.extend_from_slice(&name(&instrument.name));
        instruments.extend_from_slice(&index(*start));
    }
    instruments.extend_from_slice(&name("EOI"));
    instruments.extend_from_slice(&index(instrument_starts.last().copied().unwrap_or(0)));
    let body = [
        info(definition),
        list(b"sdta", &[chunk(b"smpl", &layout.data)]),
        list(
            b"pdta",
            &[
                chunk(b"phdr", &headers),
                chunk(b"pbag", &preset_lists.bags),
                chunk(b"pmod", &preset_lists.modulators),
                chunk(b"pgen", &preset_lists.generators),
                chunk(b"inst", &instruments),
                chunk(b"ibag", &instrument_lists.bags),
                chunk(b"imod", &instrument_lists.modulators),
                chunk(b"igen", &instrument_lists.generators),
                chunk(b"shdr", &layout.headers),
            ],
        ),
    ]
    .concat();
    let mut riff = b"sfbk".to_vec();
    riff.extend_from_slice(&body);
    chunk(b"RIFF", &riff)
}

const BOMB_GENERATORS: usize = 65_535;
const BOMB_SAMPLE_POINTS: usize = 100;
const BOMB_SAMPLE_END: u32 = 50;
const BOMB_ROOT_KEY: u8 = 60;
const BOMB_INSTRUMENT_GENERATOR: u16 = 41;
const BOMB_SAMPLE_GENERATOR: u16 = 53;

pub fn zone_bomb(bag_entries: u16) -> Vec<u8> {
    let mut version = Vec::new();
    version.extend_from_slice(&VERSION_MAJOR.to_le_bytes());
    version.extend_from_slice(&VERSION_MINOR.to_le_bytes());
    let info = list(
        b"INFO",
        &[
            chunk(b"ifil", &version),
            chunk(b"isng", &text(SOUND_ENGINE)),
            chunk(b"INAM", &text("bomb")),
        ],
    );
    let sdta = list(b"sdta", &[chunk(b"smpl", &[0u8; BOMB_SAMPLE_POINTS * 2])]);
    let mut phdr = Vec::new();
    phdr.extend_from_slice(&name("P"));
    phdr.extend(std::iter::repeat_n(0u8, 18));
    phdr.extend_from_slice(&name("EOP"));
    phdr.extend(std::iter::repeat_n(0u8, 4));
    phdr.extend_from_slice(&1u16.to_le_bytes());
    phdr.extend(std::iter::repeat_n(0u8, 12));
    let mut pbag = Vec::new();
    for generator_index in [0u16, 1] {
        pbag.extend_from_slice(&generator_index.to_le_bytes());
        pbag.extend_from_slice(&0u16.to_le_bytes());
    }
    let mut pgen = BOMB_INSTRUMENT_GENERATOR.to_le_bytes().to_vec();
    pgen.extend_from_slice(&[0u8; 6]);
    let mut inst = name("I").to_vec();
    inst.extend_from_slice(&0u16.to_le_bytes());
    inst.extend_from_slice(&name("EOI"));
    inst.extend_from_slice(&bag_entries.saturating_sub(1).to_le_bytes());
    let mut ibag = Vec::new();
    for entry in 0..bag_entries {
        let generator_index = if entry % 2 == 0 {
            index(0)
        } else {
            index(BOMB_GENERATORS - 1)
        };
        ibag.extend_from_slice(&generator_index);
        ibag.extend_from_slice(&0u16.to_le_bytes());
    }
    let mut igen = Vec::with_capacity(BOMB_GENERATORS * 4 + 4);
    for _ in 0..BOMB_GENERATORS {
        igen.extend_from_slice(&BOMB_SAMPLE_GENERATOR.to_le_bytes());
        igen.extend_from_slice(&0u16.to_le_bytes());
    }
    igen.extend_from_slice(&[0u8; 4]);
    let mut shdr = name("S").to_vec();
    for value in [0, BOMB_SAMPLE_END, 0, 0, super::SAMPLE_RATE] {
        shdr.extend_from_slice(&value.to_le_bytes());
    }
    shdr.extend_from_slice(&[BOMB_ROOT_KEY, 0]);
    shdr.extend_from_slice(&0u16.to_le_bytes());
    shdr.extend_from_slice(&MONO_SAMPLE.to_le_bytes());
    shdr.extend_from_slice(&name("EOS"));
    shdr.extend(std::iter::repeat_n(0u8, 26));
    let pdta = list(
        b"pdta",
        &[
            chunk(b"phdr", &phdr),
            chunk(b"pbag", &pbag),
            chunk(b"pmod", &[0u8; MODULATOR_BYTES]),
            chunk(b"pgen", &pgen),
            chunk(b"inst", &inst),
            chunk(b"ibag", &ibag),
            chunk(b"imod", &[0u8; MODULATOR_BYTES]),
            chunk(b"igen", &igen),
            chunk(b"shdr", &shdr),
        ],
    );
    let mut riff = b"sfbk".to_vec();
    riff.extend_from_slice(&[info, sdta, pdta].concat());
    chunk(b"RIFF", &riff)
}
