#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum LoopMode {
    NoLoop,
    OneShot,
    Continuous,
    Sustain,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum OffMode {
    Fast,
    Normal,
    Time,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Trigger {
    Attack,
    First,
    Legato,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct VelocityPoint {
    pub velocity: u8,
    pub gain: f32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct CcRange {
    pub controller: u8,
    pub low: u8,
    pub high: u8,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Envelope {
    pub attack: f32,
    pub hold: f32,
    pub decay: f32,
    pub sustain: f32,
    pub release: f32,
}

#[derive(Debug, Clone, PartialEq)]
pub struct SfzRegion {
    pub sample: String,
    pub lokey: u8,
    pub hikey: u8,
    pub lovel: u8,
    pub hivel: u8,
    pub pitch_keycenter: u8,
    pub tune: i32,
    pub transpose: i32,
    pub pitch_keytrack: i32,
    pub volume: f32,
    pub amplitude: f32,
    pub pan: f32,
    pub amp_veltrack: f32,
    pub amp_velcurve: Vec<VelocityPoint>,
    pub ampeg: Envelope,
    pub loop_mode: LoopMode,
    pub loop_start: Option<u64>,
    pub loop_end: Option<u64>,
    pub offset: u64,
    pub end: Option<u64>,
    pub group: u32,
    pub off_by: u32,
    pub off_mode: OffMode,
    pub off_time: f32,
    pub lorand: f32,
    pub hirand: f32,
    pub seq_length: u8,
    pub seq_position: u8,
    pub trigger: Trigger,
    pub sw_last: Option<u8>,
    pub cc_ranges: Vec<CcRange>,
}

impl SfzRegion {
    pub fn accepts(&self, key: u8, velocity: u8) -> bool {
        (self.lokey..=self.hikey).contains(&key) && (self.lovel..=self.hivel).contains(&velocity)
    }

    pub fn velocity_gain(&self, velocity: u8) -> f32 {
        let value = f32::from(velocity.min(127)) / 127.0;
        let curve = if self.amp_velcurve.is_empty() {
            value * value
        } else {
            curve_at(&self.amp_velcurve, velocity)
        };
        let track = self.amp_veltrack / 100.0;
        ((1.0 - track) + track * curve).max(0.0)
    }
}

fn curve_at(points: &[VelocityPoint], velocity: u8) -> f32 {
    let mut previous = VelocityPoint {
        velocity: 0,
        gain: 0.0,
    };
    for point in points {
        if point.velocity >= velocity {
            if point.velocity == previous.velocity {
                return point.gain;
            }
            let span = f32::from(point.velocity - previous.velocity);
            let along = f32::from(velocity - previous.velocity) / span;
            return previous.gain + (point.gain - previous.gain) * along;
        }
        previous = *point;
    }
    previous.gain
}
