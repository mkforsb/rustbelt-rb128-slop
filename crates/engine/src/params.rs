//! Parameter metadata shared by the engine (for mapping) and the UI (for
//! knobs, labels and randomization). Every parameter travels over the wire as a
//! normalized `0..=1` knob position; [`ParamSpec::to_real`] maps it to units.

use crate::dsp::dj_filter_shape;

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Map {
    Lin(f32, f32),
    Exp(f32, f32),
    Choice(&'static [&'static str]),
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Unit {
    None,
    Hz,
    Ms,
    Sec,
    Pct,
    Semi,
    Db,
    Cents,
    Note,
    Pan,
    Ratio,
    Filter,
    Choice,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ParamSpec {
    pub name: &'static str,
    pub map: Map,
    pub unit: Unit,
    /// Default in real units (or choice index).
    pub default: f32,
    /// Normalized range used by the randomize button.
    pub rnd: (f32, f32),
}

impl ParamSpec {
    pub const fn new(name: &'static str, map: Map, unit: Unit, default: f32) -> Self {
        Self {
            name,
            map,
            unit,
            default,
            rnd: (0.0, 1.0),
        }
    }

    pub const fn rnd(mut self, lo: f32, hi: f32) -> Self {
        self.rnd = (lo, hi);
        self
    }

    pub const fn no_rnd(self) -> Self {
        self.rnd(-1.0, -1.0)
    }

    pub fn randomizable(&self) -> bool {
        self.rnd.0 >= 0.0
    }

    pub fn to_real(&self, n: f32) -> f32 {
        let n = n.clamp(0.0, 1.0);
        match self.map {
            Map::Lin(a, b) => a + (b - a) * n,
            Map::Exp(a, b) => a * (b / a).powf(n),
            Map::Choice(c) => ((n * c.len() as f32) as usize).min(c.len() - 1) as f32,
        }
    }

    pub fn to_norm(&self, real: f32) -> f32 {
        match self.map {
            Map::Lin(a, b) => ((real - a) / (b - a)).clamp(0.0, 1.0),
            Map::Exp(a, b) => ((real / a).ln() / (b / a).ln()).clamp(0.0, 1.0),
            Map::Choice(c) => ((real + 0.5) / c.len() as f32).clamp(0.0, 1.0),
        }
    }

    pub fn default_norm(&self) -> f32 {
        self.to_norm(self.default)
    }

    pub fn steps(&self) -> Option<usize> {
        match self.map {
            Map::Choice(c) => Some(c.len()),
            _ => None,
        }
    }

    pub fn format(&self, n: f32) -> String {
        let v = self.to_real(n);
        match self.unit {
            Unit::None => format!("{v:.2}"),
            Unit::Hz => {
                if v >= 1000.0 {
                    format!("{:.1}k", v / 1000.0)
                } else {
                    format!("{v:.0}Hz")
                }
            }
            Unit::Ms => {
                if v >= 1000.0 {
                    format!("{:.2}s", v / 1000.0)
                } else if v < 10.0 {
                    format!("{v:.1}ms")
                } else {
                    format!("{v:.0}ms")
                }
            }
            Unit::Sec => format!("{v:.1}s"),
            Unit::Pct => format!("{:.0}%", v * 100.0),
            Unit::Semi => {
                if v.abs() < 0.05 {
                    "0st".into()
                } else {
                    format!("{v:+.1}st")
                }
            }
            Unit::Db => {
                if matches!(self.map, Map::Lin(a, _) if a <= -48.0) && v <= -47.9 {
                    "-inf".into()
                } else {
                    format!("{v:+.1}dB")
                }
            }
            Unit::Cents => format!("{v:.0}ct"),
            Unit::Note => note_name(48 + v.round() as i32),
            Unit::Pan => {
                let p = (v * 50.0).round() as i32;
                match p {
                    0 => "C".into(),
                    p if p < 0 => format!("L{}", -p),
                    p => format!("R{p}"),
                }
            }
            Unit::Ratio => format!("{v:.1}:1"),
            Unit::Filter => {
                let s = dj_filter_shape(v);
                match s.mode {
                    0 => "OFF".into(),
                    m => {
                        let f = if s.cutoff >= 1000.0 {
                            format!("{:.1}k", s.cutoff / 1000.0)
                        } else {
                            format!("{:.0}", s.cutoff)
                        };
                        format!("{} {f}", if m < 0 { "LP" } else { "HP" })
                    }
                }
            }
            Unit::Choice => match self.map {
                Map::Choice(c) => c[v as usize].to_string(),
                _ => format!("{v}"),
            },
        }
    }
}

pub fn note_name(midi: i32) -> String {
    const N: [&str; 12] = [
        "C", "C#", "D", "D#", "E", "F", "F#", "G", "G#", "A", "A#", "B",
    ];
    format!(
        "{}{}",
        N[midi.rem_euclid(12) as usize],
        midi.div_euclid(12) - 1
    )
}

use Map::*;
use Unit as U;

// ---------------------------------------------------------------- channels

pub const NUM_CH: usize = 13;
pub const MAX_PARAMS: usize = 16;
pub const KICK: usize = 0;
pub const CLOSED_HAT: usize = 1;
pub const OPEN_HAT: usize = 2;
pub const STAB: usize = 12;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Kick,
    Hat,
    Ride,
    Snare,
    Clap,
    Perc,
    Stab,
}

pub struct ChannelDef {
    pub name: &'static str,
    pub short: &'static str,
    pub kind: Kind,
    pub specs: &'static [ParamSpec],
}

// Kick
pub const K_TUNE: usize = 0;
pub const K_BEND: usize = 1;
pub const K_BEND_T: usize = 2;
pub const K_DECAY: usize = 3;
pub const K_HIT: usize = 4;
pub const K_HIT_TONE: usize = 5;
pub const K_SUB: usize = 6;
pub const K_SUB_DECAY: usize = 7;
pub const K_DRIVE: usize = 8;
pub const K_RMB: usize = 9;
pub const K_RMB_DECAY: usize = 10;
pub const K_RMB_LO: usize = 11;
pub const K_RMB_HI: usize = 12;
pub const K_RMB_DRIVE: usize = 13;
pub const K_RMB_DUCK: usize = 14;

pub static KICK_SPECS: [ParamSpec; 15] = [
    ParamSpec::new("Tune", Exp(30.0, 90.0), U::Hz, 47.0).rnd(0.15, 0.6),
    ParamSpec::new("Bend", Lin(0.0, 48.0), U::Semi, 28.0).rnd(0.3, 0.8),
    ParamSpec::new("Bend T", Exp(3.0, 200.0), U::Ms, 28.0).rnd(0.2, 0.65),
    ParamSpec::new("Decay", Exp(60.0, 2500.0), U::Ms, 520.0).rnd(0.3, 0.75),
    ParamSpec::new("Hit", Lin(0.0, 1.0), U::Pct, 0.35),
    ParamSpec::new("Hit Tone", Exp(400.0, 9000.0), U::Hz, 2800.0),
    ParamSpec::new("Sub", Lin(0.0, 1.0), U::Pct, 0.45).rnd(0.1, 0.8),
    ParamSpec::new("Sub Dec", Exp(60.0, 3000.0), U::Ms, 650.0).rnd(0.2, 0.75),
    ParamSpec::new("Drive", Lin(0.0, 1.0), U::Pct, 0.3).rnd(0.0, 0.8),
    ParamSpec::new("Rumble", Lin(0.0, 1.0), U::Pct, 0.45).rnd(0.0, 0.85),
    ParamSpec::new("Rmb Dec", Exp(0.3, 10.0), U::Sec, 2.8),
    ParamSpec::new("Rmb Lo", Exp(20.0, 250.0), U::Hz, 38.0).rnd(0.0, 0.6),
    ParamSpec::new("Rmb Hi", Exp(60.0, 1200.0), U::Hz, 170.0).rnd(0.1, 0.7),
    ParamSpec::new("Rmb Drv", Lin(0.0, 1.0), U::Pct, 0.45),
    ParamSpec::new("Rmb Duck", Lin(0.0, 1.0), U::Pct, 0.85).rnd(0.5, 1.0),
];

// Metallic (hats / ride)
pub const M_TUNE: usize = 0;
pub const M_ATTACK: usize = 1;
pub const M_DECAY: usize = 2;
pub const M_TONE: usize = 3;
pub const M_NOISE: usize = 4;
pub const M_BELL: usize = 5;

const fn hat_specs(decay: f32, dmin: f32, dmax: f32) -> [ParamSpec; 5] {
    [
        ParamSpec::new("Pitch", Lin(-12.0, 12.0), U::Semi, 0.0).rnd(0.2, 0.8),
        ParamSpec::new("Attack", Exp(0.1, 40.0), U::Ms, 0.2).rnd(0.0, 0.5),
        ParamSpec::new("Decay", Exp(dmin, dmax), U::Ms, decay),
        ParamSpec::new("Tone", Exp(2500.0, 14000.0), U::Hz, 7500.0),
        ParamSpec::new("Noise", Lin(0.0, 1.0), U::Pct, 0.25),
    ]
}

pub static CH_SPECS: [ParamSpec; 5] = hat_specs(45.0, 8.0, 500.0);
pub static OH_SPECS: [ParamSpec; 5] = hat_specs(380.0, 40.0, 2500.0);
pub static RIDE_SPECS: [ParamSpec; 6] = [
    ParamSpec::new("Pitch", Lin(-12.0, 12.0), U::Semi, 0.0).rnd(0.2, 0.8),
    ParamSpec::new("Attack", Exp(0.1, 40.0), U::Ms, 0.3).rnd(0.0, 0.5),
    ParamSpec::new("Decay", Exp(150.0, 5000.0), U::Ms, 1400.0),
    ParamSpec::new("Tone", Exp(1500.0, 12000.0), U::Hz, 5200.0),
    ParamSpec::new("Noise", Lin(0.0, 1.0), U::Pct, 0.12),
    ParamSpec::new("Bell", Lin(0.0, 1.0), U::Pct, 0.4),
];

// Snare
pub const S_TUNE: usize = 0;
pub const S_BEND: usize = 1;
pub const S_DECAY: usize = 2;
pub const S_SNAPPY: usize = 3;
pub const S_NDECAY: usize = 4;
pub const S_NTONE: usize = 5;

pub static SNARE_SPECS: [ParamSpec; 6] = [
    ParamSpec::new("Tune", Exp(100.0, 400.0), U::Hz, 185.0),
    ParamSpec::new("Bend", Lin(0.0, 24.0), U::Semi, 5.0),
    ParamSpec::new("Decay", Exp(20.0, 600.0), U::Ms, 110.0),
    ParamSpec::new("Snappy", Lin(0.0, 1.0), U::Pct, 0.6),
    ParamSpec::new("Snap Dec", Exp(20.0, 1000.0), U::Ms, 170.0),
    ParamSpec::new("Snap Tone", Exp(800.0, 12000.0), U::Hz, 4500.0),
];

// Clap
pub const C_TONE: usize = 0;
pub const C_Q: usize = 1;
pub const C_SPREAD: usize = 2;
pub const C_BURSTS: usize = 3;
pub const C_DECAY: usize = 4;

pub static CLAP_SPECS: [ParamSpec; 5] = [
    ParamSpec::new("Tone", Exp(500.0, 5000.0), U::Hz, 1250.0),
    ParamSpec::new("Reso", Exp(0.4, 6.0), U::None, 1.2),
    ParamSpec::new("Spread", Exp(3.0, 30.0), U::Ms, 9.0),
    ParamSpec::new(
        "Bursts",
        Choice(&["1", "2", "3", "4", "5", "6"]),
        U::Choice,
        3.0,
    ),
    ParamSpec::new("Decay", Exp(40.0, 1500.0), U::Ms, 260.0),
];

// Tonal percussion (toms, congas)
pub const T_TUNE: usize = 0;
pub const T_BEND: usize = 1;
pub const T_BEND_T: usize = 2;
pub const T_DECAY: usize = 3;
pub const T_NOISE: usize = 4;

const fn tom_specs(tune: f32) -> [ParamSpec; 5] {
    [
        ParamSpec::new("Tune", Exp(40.0, 420.0), U::Hz, tune),
        ParamSpec::new("Bend", Lin(0.0, 24.0), U::Semi, 8.0),
        ParamSpec::new("Bend T", Exp(5.0, 400.0), U::Ms, 70.0),
        ParamSpec::new("Decay", Exp(40.0, 2000.0), U::Ms, 380.0),
        ParamSpec::new("Noise", Lin(0.0, 1.0), U::Pct, 0.18),
    ]
}

const fn conga_specs(tune: f32) -> [ParamSpec; 5] {
    [
        ParamSpec::new("Tune", Exp(120.0, 900.0), U::Hz, tune),
        ParamSpec::new("Bend", Lin(0.0, 24.0), U::Semi, 3.0),
        ParamSpec::new("Bend T", Exp(2.0, 200.0), U::Ms, 14.0),
        ParamSpec::new("Decay", Exp(20.0, 1200.0), U::Ms, 170.0),
        ParamSpec::new("Slap", Lin(0.0, 1.0), U::Pct, 0.35),
    ]
}

pub static LT_SPECS: [ParamSpec; 5] = tom_specs(85.0);
pub static MT_SPECS: [ParamSpec; 5] = tom_specs(125.0);
pub static HT_SPECS: [ParamSpec; 5] = tom_specs(180.0);
pub static LC_SPECS: [ParamSpec; 5] = conga_specs(210.0);
pub static MC_SPECS: [ParamSpec; 5] = conga_specs(310.0);
pub static HC_SPECS: [ParamSpec; 5] = conga_specs(440.0);

// FM stab: three freely tuned voices (semitones relative to C3) plus a
// global transpose.
pub const F_TUNE: usize = 0;
pub const F_PITCH1: usize = 1;
pub const F_PITCH2: usize = 2;
pub const F_PITCH3: usize = 3;
pub const F_RATIO: usize = 4;
pub const F_INDEX: usize = 5;
pub const F_MOD_DEC: usize = 6;
pub const F_FEEDBACK: usize = 7;
pub const F_ATTACK: usize = 8;
pub const F_DECAY: usize = 9;
pub const F_CUTOFF: usize = 10;
pub const F_DETUNE: usize = 11;

pub const RATIO_NAMES: &[&str] = &["0.5", "1", "1.5", "2", "3", "3.5", "4", "5", "7", "9"];
pub const RATIOS: [f32; 10] = [0.5, 1.0, 1.5, 2.0, 3.0, 3.5, 4.0, 5.0, 7.0, 9.0];

pub static STAB_SPECS: [ParamSpec; 12] = [
    ParamSpec::new("Tune", Lin(-12.0, 12.0), U::Semi, 0.0).rnd(0.3, 0.7),
    ParamSpec::new("Voice 1", Lin(-24.0, 24.0), U::Semi, 0.0).rnd(0.42, 0.58),
    ParamSpec::new("Voice 2", Lin(-24.0, 24.0), U::Semi, 3.0).rnd(0.5, 0.72),
    ParamSpec::new("Voice 3", Lin(-24.0, 24.0), U::Semi, 7.0).rnd(0.55, 0.8),
    ParamSpec::new("Ratio", Choice(RATIO_NAMES), U::Choice, 1.0),
    ParamSpec::new("FM Amt", Lin(0.0, 10.0), U::None, 2.6).rnd(0.0, 0.7),
    ParamSpec::new("FM Dec", Exp(5.0, 2000.0), U::Ms, 160.0),
    ParamSpec::new("Fdbk", Lin(0.0, 1.0), U::Pct, 0.15).rnd(0.0, 0.6),
    ParamSpec::new("Attack", Exp(0.5, 300.0), U::Ms, 1.0).rnd(0.0, 0.5),
    ParamSpec::new("Decay", Exp(30.0, 3000.0), U::Ms, 280.0).rnd(0.1, 0.8),
    ParamSpec::new("Cutoff", Exp(150.0, 16000.0), U::Hz, 4200.0).rnd(0.3, 0.95),
    ParamSpec::new("Detune", Lin(0.0, 40.0), U::Cents, 7.0),
];

pub static CHANNELS: [ChannelDef; NUM_CH] = [
    ChannelDef {
        name: "Kick",
        short: "BD",
        kind: Kind::Kick,
        specs: &KICK_SPECS,
    },
    ChannelDef {
        name: "Closed Hat",
        short: "CH",
        kind: Kind::Hat,
        specs: &CH_SPECS,
    },
    ChannelDef {
        name: "Open Hat",
        short: "OH",
        kind: Kind::Hat,
        specs: &OH_SPECS,
    },
    ChannelDef {
        name: "Ride",
        short: "RD",
        kind: Kind::Ride,
        specs: &RIDE_SPECS,
    },
    ChannelDef {
        name: "Snare",
        short: "SD",
        kind: Kind::Snare,
        specs: &SNARE_SPECS,
    },
    ChannelDef {
        name: "Clap",
        short: "CP",
        kind: Kind::Clap,
        specs: &CLAP_SPECS,
    },
    ChannelDef {
        name: "Low Tom",
        short: "LT",
        kind: Kind::Perc,
        specs: &LT_SPECS,
    },
    ChannelDef {
        name: "Mid Tom",
        short: "MT",
        kind: Kind::Perc,
        specs: &MT_SPECS,
    },
    ChannelDef {
        name: "High Tom",
        short: "HT",
        kind: Kind::Perc,
        specs: &HT_SPECS,
    },
    ChannelDef {
        name: "Low Conga",
        short: "LC",
        kind: Kind::Perc,
        specs: &LC_SPECS,
    },
    ChannelDef {
        name: "Mid Conga",
        short: "MC",
        kind: Kind::Perc,
        specs: &MC_SPECS,
    },
    ChannelDef {
        name: "High Conga",
        short: "HC",
        kind: Kind::Perc,
        specs: &HC_SPECS,
    },
    ChannelDef {
        name: "FM Stab",
        short: "ST",
        kind: Kind::Stab,
        specs: &STAB_SPECS,
    },
];

// ------------------------------------------------------------------- mixer

pub const MIX_LEVEL: usize = 0;
pub const MIX_PAN: usize = 1;
pub const MIX_EQ_LO: usize = 2;
pub const MIX_EQ_MID: usize = 3;
pub const MIX_EQ_HI: usize = 4;
pub const MIX_DLY: usize = 5;
pub const MIX_REV: usize = 6;
pub const NUM_MIX: usize = 7;

pub static MIX_SPECS: [ParamSpec; NUM_MIX] = [
    ParamSpec::new("Level", Lin(-48.0, 6.0), U::Db, -6.0),
    ParamSpec::new("Pan", Lin(-1.0, 1.0), U::Pan, 0.0),
    ParamSpec::new("Lo", Lin(-15.0, 15.0), U::Db, 0.0),
    ParamSpec::new("Mid", Lin(-15.0, 15.0), U::Db, 0.0),
    ParamSpec::new("Hi", Lin(-15.0, 15.0), U::Db, 0.0),
    ParamSpec::new("Delay", Lin(0.0, 1.0), U::Pct, 0.0),
    ParamSpec::new("Reverb", Lin(0.0, 1.0), U::Pct, 0.0),
];

// ------------------------------------------------------------------ master

pub const DLY_TIME: usize = 0;
pub const DLY_FB: usize = 1;
pub const DLY_HICUT: usize = 2;
pub const DLY_LOCUT: usize = 3;
pub const DLY_WIDTH: usize = 4;
pub const DLY_LEVEL: usize = 5;
pub const REV_DECAY: usize = 6;
pub const REV_DAMP: usize = 7;
pub const REV_PRE: usize = 8;
pub const REV_LOCUT: usize = 9;
pub const REV_LEVEL: usize = 10;
pub const DUCK_THRESH: usize = 11;
pub const DUCK_RATIO: usize = 12;
pub const DUCK_ATTACK: usize = 13;
pub const DUCK_RELEASE: usize = 14;
pub const FILTER: usize = 15;
pub const FILTER_RES: usize = 16;
pub const LIM_MAKEUP: usize = 17;
pub const LIM_CEIL: usize = 18;
pub const LIM_RELEASE: usize = 19;
pub const MASTER_VOL: usize = 20;
pub const NUM_MASTER: usize = 21;

pub const DELAY_TIME_NAMES: &[&str] = &["1/32", "1/16", "1/8T", "1/8", "3/16", "1/4", "3/8", "1/2"];
/// Delay times in 16th-note steps.
pub const DELAY_TIME_STEPS: [f32; 8] = [0.5, 1.0, 4.0 / 3.0, 2.0, 3.0, 4.0, 6.0, 8.0];

pub static MASTER_SPECS: [ParamSpec; NUM_MASTER] = [
    ParamSpec::new("Time", Choice(DELAY_TIME_NAMES), U::Choice, 4.0),
    ParamSpec::new("Fdbk", Lin(0.0, 0.95), U::Pct, 0.45),
    ParamSpec::new("Hi Cut", Exp(500.0, 16000.0), U::Hz, 4500.0),
    ParamSpec::new("Lo Cut", Exp(20.0, 2000.0), U::Hz, 280.0),
    ParamSpec::new("Ping", Lin(0.0, 1.0), U::Pct, 0.85),
    ParamSpec::new("Return", Lin(0.0, 1.0), U::Pct, 0.75),
    ParamSpec::new("Decay", Exp(0.2, 15.0), U::Sec, 3.2),
    ParamSpec::new("Damp", Exp(800.0, 16000.0), U::Hz, 5500.0),
    ParamSpec::new("Pre", Lin(0.0, 200.0), U::Ms, 18.0),
    ParamSpec::new("Lo Cut", Exp(20.0, 2000.0), U::Hz, 320.0),
    ParamSpec::new("Return", Lin(0.0, 1.0), U::Pct, 0.75),
    ParamSpec::new("Thresh", Lin(-48.0, 0.0), U::Db, -22.0),
    ParamSpec::new("Ratio", Exp(1.0, 30.0), U::Ratio, 4.0),
    ParamSpec::new("Attack", Exp(0.1, 50.0), U::Ms, 1.5),
    ParamSpec::new("Release", Exp(10.0, 1000.0), U::Ms, 170.0),
    ParamSpec::new("Filter", Lin(-1.0, 1.0), U::Filter, 0.0),
    ParamSpec::new("Reso", Lin(0.0, 1.0), U::Pct, 0.3),
    ParamSpec::new("Makeup", Lin(0.0, 24.0), U::Db, 0.0),
    ParamSpec::new("Ceiling", Lin(-12.0, 0.0), U::Db, -0.3),
    ParamSpec::new("Release", Exp(5.0, 1000.0), U::Ms, 90.0),
    ParamSpec::new("Volume", Lin(-48.0, 6.0), U::Db, 0.0),
];

pub const BPM_MIN: f32 = 60.0;
pub const BPM_MAX: f32 = 200.0;
pub const SWING_MAX: f32 = 75.0;
