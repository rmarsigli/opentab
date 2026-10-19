pub const SAMPLE_RATE: u32 = 44_100;

const FULL_SCALE_Q30: i64 = 1 << 30;
const UNITY_Q16: i64 = 1 << 16;
const NOISE_SEED: u32 = 0x9E37_79B9;

pub struct Waveform {
    pub points: Vec<i16>,
    pub loop_start: u32,
    pub loop_end: u32,
}

struct Noise {
    state: u32,
}

impl Noise {
    fn new(seed: u32) -> Noise {
        Noise { state: seed | 1 }
    }

    fn next(&mut self) -> i32 {
        let mut x = self.state;
        x ^= x << 13;
        x ^= x >> 17;
        x ^= x << 5;
        self.state = x;
        (x >> 16) as i32 - 32_768
    }
}

pub fn sine(phase: u32, amplitude: i32) -> i32 {
    let negative = phase >> 31 == 1;
    let t = i64::from((phase & 0x7FFF_FFFF) >> 16);
    let u = t * (32_768 - t);
    let value = i64::from(amplitude) * 16 * u / (5 * FULL_SCALE_Q30 - 4 * u);
    let value = value as i32;
    if negative { -value } else { value }
}

fn clip(value: i64) -> i16 {
    value.clamp(i64::from(i16::MIN), i64::from(i16::MAX)) as i16
}

fn periodic(
    period: u32,
    periods: u32,
    loop_first: u32,
    loop_periods: u32,
    shape: fn(u32, u32) -> i32,
) -> Waveform {
    let points = (0..period * periods)
        .map(|index| clip(i64::from(shape(index % period, period))))
        .collect();
    Waveform {
        points,
        loop_start: period * loop_first,
        loop_end: period * (loop_first + loop_periods),
    }
}

fn sine_shape(index: u32, period: u32) -> i32 {
    let phase = (u64::from(index) << 32) / u64::from(period);
    sine(phase as u32, 20_000)
}

fn saw_shape(index: u32, period: u32) -> i32 {
    let ramp = i64::from(index) * 2 * 14_000 / i64::from(period) - 14_000;
    let fundamental = i64::from(sine_shape(index, period)) / 4;
    (ramp / 2 + fundamental) as i32
}

pub fn sine_loop() -> Waveform {
    periodic(100, 8, 2, 4, sine_shape)
}

pub fn saw_bass() -> Waveform {
    periodic(535, 4, 1, 2, saw_shape)
}

pub fn long_pad(frames: u32) -> Waveform {
    let period = 100;
    let periods = (frames / period).max(8);
    periodic(period, periods, 2, periods - 4, sine_shape)
}

pub fn pluck() -> Waveform {
    let period = 200usize;
    let frames = 33_075usize;
    let mut noise = Noise::new(NOISE_SEED);
    let mut ring: Vec<i64> = Vec::with_capacity(period);
    let mut smooth = 0i64;
    for index in 0..period {
        let raw = i64::from(noise.next()) / 2;
        let triangle = if index < period / 3 {
            i64::try_from(index).unwrap_or(0) * 18_000 / (period as i64 / 3)
        } else {
            i64::try_from(period - index).unwrap_or(0) * 18_000 / (period as i64 * 2 / 3)
        };
        smooth += (raw + triangle - smooth) / 3;
        ring.push(smooth);
    }
    let mean = ring.iter().sum::<i64>() / period as i64;
    for cell in &mut ring {
        *cell -= mean;
    }
    let mut points = Vec::with_capacity(frames);
    let mut position = 0usize;
    for _ in 0..frames {
        let next = (position + 1) % period;
        let current = ring[position];
        points.push(clip(current));
        ring[position] = (current + ring[next]) * 65_340 / (2 * UNITY_Q16);
        position = next;
    }
    Waveform {
        points,
        loop_start: 0,
        loop_end: 0,
    }
}

fn decay(amplitude_q16: &mut i64, factor_q16: i64) -> i64 {
    *amplitude_q16 = *amplitude_q16 * factor_q16 / UNITY_Q16;
    *amplitude_q16 / UNITY_Q16
}

fn increment(hz: u64) -> i64 {
    ((hz << 32) / u64::from(SAMPLE_RATE)) as i64
}

pub fn kick() -> Waveform {
    let mut amplitude = 30_000 * UNITY_Q16;
    let mut sweep = UNITY_Q16;
    let (start, end) = (increment(150), increment(48));
    let mut phase = 0u32;
    let points = (0..15_435)
        .map(|_| {
            sweep = sweep * 65_200 / UNITY_Q16;
            phase = phase.wrapping_add((end + (start - end) * sweep / UNITY_Q16) as u32);
            let level = decay(&mut amplitude, 65_507);
            clip(i64::from(sine(phase, 32_767)) * level / 32_767)
        })
        .collect();
    one_shot(points)
}

pub fn snare() -> Waveform {
    snare_seeded(NOISE_SEED ^ 0x5555)
}

pub fn snare_variant() -> Waveform {
    snare_seeded(NOISE_SEED ^ 0x3C3C)
}

fn snare_seeded(seed: u32) -> Waveform {
    let mut noise = Noise::new(seed);
    let mut noise_level = 22_000 * UNITY_Q16;
    let mut tone_level = 14_000 * UNITY_Q16;
    let step = increment(190) as u32;
    let mut phase = 0u32;
    let points = (0..11_025)
        .map(|_| {
            phase = phase.wrapping_add(step);
            let body = i64::from(sine(phase, 32_767)) * decay(&mut tone_level, 65_470) / 32_767;
            let rattle = i64::from(noise.next()) * decay(&mut noise_level, 65_495) / 32_768;
            clip(body + rattle)
        })
        .collect();
    one_shot(points)
}

pub fn hat() -> Waveform {
    noise_burst(NOISE_SEED ^ 0xAAAA, 4_410, 65_433)
}

pub fn open_hat() -> Waveform {
    noise_burst(NOISE_SEED ^ 0x0F0F, 22_050, 65_521)
}

fn noise_burst(seed: u32, frames: u32, factor_q16: i64) -> Waveform {
    let mut noise = Noise::new(seed);
    let mut level = 20_000 * UNITY_Q16;
    let mut previous = 0i64;
    let points = (0..frames)
        .map(|_| {
            let current = i64::from(noise.next());
            let high = (current - previous) / 2;
            previous = current;
            clip(high * decay(&mut level, factor_q16) / 32_768)
        })
        .collect();
    one_shot(points)
}

fn one_shot(points: Vec<i16>) -> Waveform {
    Waveform {
        points,
        loop_start: 0,
        loop_end: 0,
    }
}
