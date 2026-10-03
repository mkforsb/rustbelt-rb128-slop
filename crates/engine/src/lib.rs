//! RUSTBELT RB-128 engine: voices, sequencer, mixer and master effects.
//!
//! Platform-agnostic. The UI talks to it exclusively through [`Command`]s and
//! reads back a [`Status`] snapshot, which keeps the web (AudioWorklet) and
//! desktop (PulseAudio thread) backends identical.

pub mod dsp;
pub mod fx;
pub mod params;
pub mod seq;
pub mod voices;

use dsp::*;
use fx::*;
use params::*;
use seq::*;
use voices::Voice;

// ------------------------------------------------------------ protocol

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Command {
    Play,
    Pause,
    Stop,
    Bpm(f32),
    /// Swing in percent, 50 (straight) ..= 75.
    Swing(f32),
    SynthParam {
        ch: usize,
        idx: usize,
        norm: f32,
    },
    MixParam {
        ch: usize,
        idx: usize,
        norm: f32,
    },
    Mute {
        ch: usize,
        on: bool,
    },
    Solo {
        ch: usize,
        on: bool,
    },
    MasterParam {
        idx: usize,
        norm: f32,
    },
    Step {
        ch: usize,
        var: usize,
        step: usize,
        data: Step,
    },
    TrackLen {
        ch: usize,
        var: usize,
        len: usize,
    },
    QueueVar(usize),
    Trigger {
        ch: usize,
        vel: f32,
    },
}

pub type Wire = [f64; 8];

impl Command {
    pub fn encode(&self) -> Wire {
        use Command::*;
        let b = |v: bool| if v { 1.0 } else { 0.0 };
        match *self {
            Play => [1.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0],
            Pause => [2.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0],
            Stop => [3.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0],
            Bpm(v) => [4.0, v as f64, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0],
            Swing(v) => [5.0, v as f64, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0],
            SynthParam { ch, idx, norm } => {
                [6.0, ch as f64, idx as f64, norm as f64, 0.0, 0.0, 0.0, 0.0]
            }
            MixParam { ch, idx, norm } => {
                [7.0, ch as f64, idx as f64, norm as f64, 0.0, 0.0, 0.0, 0.0]
            }
            Mute { ch, on } => [8.0, ch as f64, b(on), 0.0, 0.0, 0.0, 0.0, 0.0],
            Solo { ch, on } => [9.0, ch as f64, b(on), 0.0, 0.0, 0.0, 0.0, 0.0],
            MasterParam { idx, norm } => [10.0, idx as f64, norm as f64, 0.0, 0.0, 0.0, 0.0, 0.0],
            Step {
                ch,
                var,
                step,
                data,
            } => [
                11.0,
                ch as f64,
                var as f64,
                step as f64,
                b(data.on),
                data.vel as f64,
                data.pitch as f64,
                data.prob as f64,
            ],
            TrackLen { ch, var, len } => {
                [12.0, ch as f64, var as f64, len as f64, 0.0, 0.0, 0.0, 0.0]
            }
            QueueVar(v) => [13.0, v as f64, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0],
            Trigger { ch, vel } => [14.0, ch as f64, vel as f64, 0.0, 0.0, 0.0, 0.0, 0.0],
        }
    }

    /// Decodes and validates; returns `None` for anything malformed.
    pub fn decode(w: &[f64]) -> Option<Command> {
        use Command::*;
        if w.len() < 8 {
            return None;
        }
        let u = |i: usize, max: usize| -> Option<usize> {
            let v = w[i];
            (v >= 0.0 && (v as usize) < max).then_some(v as usize)
        };
        let f = |i: usize| w[i] as f32;
        let on = |i: usize| w[i] != 0.0;
        Some(match w[0] as u32 {
            1 => Play,
            2 => Pause,
            3 => Stop,
            4 => Bpm(f(1).clamp(BPM_MIN, BPM_MAX)),
            5 => Swing(f(1).clamp(50.0, SWING_MAX)),
            6 => SynthParam {
                ch: u(1, NUM_CH)?,
                idx: u(2, MAX_PARAMS)?,
                norm: f(3),
            },
            7 => MixParam {
                ch: u(1, NUM_CH)?,
                idx: u(2, NUM_MIX)?,
                norm: f(3),
            },
            8 => Mute {
                ch: u(1, NUM_CH)?,
                on: on(2),
            },
            9 => Solo {
                ch: u(1, NUM_CH)?,
                on: on(2),
            },
            10 => MasterParam {
                idx: u(1, NUM_MASTER)?,
                norm: f(2),
            },
            11 => Step {
                ch: u(1, NUM_CH)?,
                var: u(2, NUM_VARS)?,
                step: u(3, MAX_STEPS)?,
                data: seq::Step {
                    on: on(4),
                    vel: f(5).clamp(0.0, 1.0),
                    pitch: if w[6].is_finite() {
                        f(6).clamp(-PITCH_RANGE, PITCH_RANGE)
                    } else {
                        0.0
                    },
                    prob: f(7).clamp(0.0, 1.0),
                },
            },
            12 => TrackLen {
                ch: u(1, NUM_CH)?,
                var: u(2, NUM_VARS)?,
                len: u(3, MAX_STEPS + 1)?.max(1),
            },
            13 => QueueVar(u(1, NUM_VARS)?),
            14 => Trigger {
                ch: u(1, NUM_CH)?,
                vel: f(2).clamp(0.0, 1.0),
            },
            _ => return None,
        })
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Status {
    pub playing: bool,
    pub var: usize,
    pub pending: Option<usize>,
    pub tick: u32,
    pub peak: [f32; 2],
    pub duck_gr: f32,
    pub lim_gr: f32,
    /// Step each channel most recently played (-1 = none yet).
    pub steps: [i32; NUM_CH],
    pub meters: [f32; NUM_CH],
}

pub const STATUS_LEN: usize = 8 + 2 * NUM_CH;

impl Default for Status {
    fn default() -> Self {
        Self {
            playing: false,
            var: 0,
            pending: None,
            tick: 0,
            peak: [0.0; 2],
            duck_gr: 0.0,
            lim_gr: 0.0,
            steps: [-1; NUM_CH],
            meters: [0.0; NUM_CH],
        }
    }
}

impl Status {
    pub fn write(&self, out: &mut [f32; STATUS_LEN]) {
        out[0] = self.playing as u8 as f32;
        out[1] = self.var as f32;
        out[2] = self.pending.map(|v| v as f32).unwrap_or(-1.0);
        out[3] = self.tick as f32;
        out[4] = self.peak[0];
        out[5] = self.peak[1];
        out[6] = self.duck_gr;
        out[7] = self.lim_gr;
        for i in 0..NUM_CH {
            out[8 + i] = self.steps[i] as f32;
            out[8 + NUM_CH + i] = self.meters[i];
        }
    }

    pub fn read(a: &[f32]) -> Option<Status> {
        if a.len() < STATUS_LEN {
            return None;
        }
        let mut s = Status {
            playing: a[0] != 0.0,
            var: (a[1] as usize).min(NUM_VARS - 1),
            pending: (a[2] >= 0.0).then(|| (a[2] as usize).min(NUM_VARS - 1)),
            tick: a[3] as u32,
            peak: [a[4], a[5]],
            duck_gr: a[6],
            lim_gr: a[7],
            ..Default::default()
        };
        for i in 0..NUM_CH {
            s.steps[i] = a[8 + i] as i32;
            s.meters[i] = a[8 + NUM_CH + i];
        }
        Some(s)
    }
}

// ------------------------------------------------------------- engine

const BLOCK: usize = 32;

struct Channel {
    def: &'static ChannelDef,
    voice: Voice,
    real: [f32; MAX_PARAMS],
    mix: [f32; NUM_MIX],
    eq: [Biquad; 3],
    eq_on: bool,
    mute: bool,
    solo: bool,
    // smoothed gains: level*pan L, level*pan R, mono level, delay send, reverb send
    g: [f32; 5],
    target: [f32; 5],
    last: f32,
    declick: f32,
    meter: f32,
}

impl Channel {
    fn new(sr: f32, ch: usize) -> Self {
        let def = &CHANNELS[ch];
        let mut c = Self {
            def,
            voice: Voice::new(def.kind, sr, ch),
            real: [0.0; MAX_PARAMS],
            mix: [0.0; NUM_MIX],
            eq: [Biquad::default(); 3],
            eq_on: false,
            mute: false,
            solo: false,
            g: [0.0; 5],
            target: [0.0; 5],
            last: 0.0,
            declick: 0.0,
            meter: 0.0,
        };
        for (i, s) in def.specs.iter().enumerate() {
            c.real[i] = s.default;
        }
        for (i, s) in MIX_SPECS.iter().enumerate() {
            c.mix[i] = s.default;
        }
        c.voice.as_impl().configure(&c.real);
        c.update_eq(sr);
        c
    }

    fn update_eq(&mut self, sr: f32) {
        self.eq[0].lowshelf(sr, 110.0, self.mix[MIX_EQ_LO]);
        self.eq[1].peaking(sr, 1100.0, 0.7, self.mix[MIX_EQ_MID]);
        self.eq[2].highshelf(sr, 7000.0, self.mix[MIX_EQ_HI]);
        self.eq_on = [MIX_EQ_LO, MIX_EQ_MID, MIX_EQ_HI]
            .iter()
            .any(|&i| self.mix[i].abs() > 0.05);
    }

    fn trigger(&mut self, vel: f32, pitch: f32) {
        self.declick = self.last;
        self.voice.as_impl().trigger(&self.real, vel, pitch);
    }
}

pub struct Engine {
    sr: f32,
    channels: Vec<Channel>,
    pattern: Pattern,
    rng: Rng,
    // transport
    playing: bool,
    bpm: f32,
    swing: f32,
    tick: u64,
    to_tick: f64,
    bar_pos: usize,
    var: usize,
    pending: Option<usize>,
    pos: [usize; NUM_CH],
    cur_step: [i32; NUM_CH],
    // master
    master: [f32; NUM_MASTER],
    delay: Delay,
    reverb: Reverb,
    ducker: Ducker,
    filter: DjFilter,
    limiter: Limiter,
    vol: f32,
    vol_target: f32,
    smooth_k: f32,
    meter_k: f32,
    peak: [f32; 2],
    duck_gr: f32,
    lim_gr: f32,
}

impl Engine {
    pub fn new(sr: f32) -> Self {
        let mut e = Self {
            sr,
            channels: (0..NUM_CH).map(|i| Channel::new(sr, i)).collect(),
            pattern: empty_pattern(),
            rng: Rng::new(0xA5A5_1234),
            playing: false,
            bpm: 130.0,
            swing: 0.0,
            tick: 0,
            to_tick: 0.0,
            bar_pos: 0,
            var: 0,
            pending: None,
            pos: [0; NUM_CH],
            cur_step: [-1; NUM_CH],
            master: [0.0; NUM_MASTER],
            delay: Delay::new(sr),
            reverb: Reverb::new(sr),
            ducker: Ducker::new(sr),
            filter: DjFilter::new(sr),
            limiter: Limiter::new(sr),
            vol: 1.0,
            vol_target: 1.0,
            smooth_k: smooth_coef(sr, 4.0),
            meter_k: decay_coef(sr, 600.0),
            peak: [0.0; 2],
            duck_gr: 0.0,
            lim_gr: 0.0,
        };
        for (i, s) in MASTER_SPECS.iter().enumerate() {
            e.master[i] = s.default;
        }
        e.update_master();
        e.update_targets();
        e.channels.iter_mut().for_each(|c| c.g = c.target);
        e
    }

    pub fn sample_rate(&self) -> f32 {
        self.sr
    }

    /// Load a whole pattern (used by tests and the offline renderer; the UI
    /// streams individual `Step` commands instead).
    pub fn load_pattern(&mut self, p: Pattern) {
        self.pattern = p;
    }

    fn samples_per_step(&self) -> f64 {
        self.sr as f64 * 60.0 / self.bpm as f64 / 4.0
    }

    fn update_master(&mut self) {
        let m = &self.master;
        let steps = DELAY_TIME_STEPS[(m[DLY_TIME] as usize).min(DELAY_TIME_STEPS.len() - 1)];
        self.delay
            .set_time_samples(steps * self.samples_per_step() as f32);
        self.delay.feedback = m[DLY_FB];
        self.delay.width = m[DLY_WIDTH];
        self.delay.level = m[DLY_LEVEL];
        self.delay.set_filters(m[DLY_HICUT], m[DLY_LOCUT]);
        self.reverb
            .set(m[REV_DECAY], m[REV_DAMP], m[REV_PRE], m[REV_LOCUT]);
        self.reverb.level = m[REV_LEVEL];
        self.ducker.set(
            m[DUCK_THRESH],
            m[DUCK_RATIO],
            m[DUCK_ATTACK],
            m[DUCK_RELEASE],
        );
        self.filter.knob = m[FILTER];
        self.filter.set_res(m[FILTER_RES]);
        self.limiter.set(m[LIM_MAKEUP], m[LIM_CEIL], m[LIM_RELEASE]);
        self.vol_target = fader_gain(m[MASTER_VOL]);
    }

    fn update_targets(&mut self) {
        for c in &mut self.channels {
            let lvl = fader_gain(c.mix[MIX_LEVEL]);
            let pan = (c.mix[MIX_PAN] + 1.0) * std::f32::consts::FRAC_PI_4;
            c.target = [
                lvl * pan.cos() * std::f32::consts::SQRT_2,
                lvl * pan.sin() * std::f32::consts::SQRT_2,
                lvl,
                lvl * c.mix[MIX_DLY],
                lvl * c.mix[MIX_REV],
            ];
        }
    }

    pub fn handle(&mut self, cmd: Command) {
        use Command::*;
        match cmd {
            Play => {
                if !self.playing {
                    self.playing = true;
                    self.to_tick = 0.0;
                }
            }
            Pause => self.playing = false,
            Stop => {
                self.playing = false;
                self.rewind();
                if let Some(v) = self.pending.take() {
                    self.var = v;
                }
            }
            Bpm(b) => {
                // keep the phase within the current step proportional
                let old = self.samples_per_step();
                self.bpm = b.clamp(BPM_MIN, BPM_MAX);
                self.to_tick *= self.samples_per_step() / old;
                self.update_master();
            }
            Swing(s) => self.swing = (s.clamp(50.0, SWING_MAX) - 50.0) / 50.0,
            SynthParam { ch, idx, norm } => {
                let c = &mut self.channels[ch];
                if let Some(spec) = c.def.specs.get(idx) {
                    c.real[idx] = spec.to_real(norm);
                    c.voice.as_impl().configure(&c.real);
                }
            }
            MixParam { ch, idx, norm } => {
                let c = &mut self.channels[ch];
                c.mix[idx] = MIX_SPECS[idx].to_real(norm);
                if (MIX_EQ_LO..=MIX_EQ_HI).contains(&idx) {
                    c.update_eq(self.sr);
                }
                self.update_targets();
            }
            Mute { ch, on } => self.channels[ch].mute = on,
            Solo { ch, on } => self.channels[ch].solo = on,
            MasterParam { idx, norm } => {
                self.master[idx] = MASTER_SPECS[idx].to_real(norm);
                self.update_master();
            }
            Step {
                ch,
                var,
                step,
                data,
            } => self.pattern[ch][var].steps[step] = data,
            TrackLen { ch, var, len } => {
                self.pattern[ch][var].len = len.clamp(1, MAX_STEPS);
                if var == self.var {
                    self.pos[ch] %= self.pattern[ch][var].len;
                }
            }
            QueueVar(v) => {
                if v == self.var {
                    self.pending = None;
                } else if self.playing {
                    self.pending = Some(v);
                } else {
                    self.var = v;
                    self.pending = None;
                    self.rewind();
                }
            }
            Trigger { ch, vel } => self.trigger(ch, vel, 0.0),
        }
    }

    fn rewind(&mut self) {
        self.pos = [0; NUM_CH];
        self.bar_pos = 0;
        self.tick = 0;
        self.to_tick = 0.0;
        self.cur_step = [-1; NUM_CH];
    }

    fn trigger(&mut self, ch: usize, vel: f32, pitch: f32) {
        if ch == CLOSED_HAT {
            self.channels[OPEN_HAT].voice.as_impl().choke();
        }
        self.channels[ch].trigger(vel, pitch);
    }

    /// The longest loop in the playing variation: variation changes wait for it.
    fn master_len(&self) -> usize {
        (0..NUM_CH)
            .map(|c| self.pattern[c][self.var].len)
            .max()
            .unwrap_or(PAGE)
            .max(1)
    }

    fn do_tick(&mut self) {
        if self.bar_pos >= self.master_len() {
            self.bar_pos = 0;
            if let Some(v) = self.pending.take() {
                self.var = v;
                self.pos = [0; NUM_CH];
            }
        }
        // Mute and solo gate the *notes*, not the audio: tails (rumble,
        // reverb, open hats...) ring out naturally.
        let any_solo = self.channels.iter().any(|c| c.solo);
        for ch in 0..NUM_CH {
            let c = &self.channels[ch];
            let plays = if any_solo { c.solo } else { !c.mute };
            let track = &self.pattern[ch][self.var];
            let len = track.len.max(1);
            let i = self.pos[ch] % len;
            let s = track.steps[i];
            self.cur_step[ch] = i as i32;
            self.pos[ch] = (i + 1) % len;
            if plays && s.on && (s.prob >= 1.0 || self.rng.unit() < s.prob) {
                self.trigger(ch, s.vel, s.pitch);
            }
        }
        self.bar_pos += 1;
        let sps = self.samples_per_step();
        let sw = self.swing as f64;
        self.to_tick += if self.tick.is_multiple_of(2) {
            sps * (1.0 + sw)
        } else {
            sps * (1.0 - sw)
        };
        self.tick += 1;
    }

    pub fn status(&self) -> Status {
        let mut meters = [0.0; NUM_CH];
        for (m, c) in meters.iter_mut().zip(&self.channels) {
            *m = c.meter;
        }
        Status {
            playing: self.playing,
            var: self.var,
            pending: self.pending,
            tick: self.tick as u32,
            peak: self.peak,
            duck_gr: self.duck_gr,
            lim_gr: self.lim_gr,
            steps: self.cur_step,
            meters,
        }
    }

    /// Render a block of stereo audio.
    pub fn render(&mut self, out_l: &mut [f32], out_r: &mut [f32]) {
        let n = out_l.len().min(out_r.len());
        let mut start = 0;
        while start < n {
            let end = (start + BLOCK).min(n);
            self.filter.update();
            for i in start..end {
                let (l, r) = self.render_sample();
                out_l[i] = l;
                out_r[i] = r;
            }
            start = end;
        }
    }

    #[inline]
    fn render_sample(&mut self) -> (f32, f32) {
        if self.playing {
            while self.to_tick <= 0.0 {
                self.do_tick();
            }
            self.to_tick -= 1.0;
        }

        let k = self.smooth_k;
        let mk = self.meter_k;
        let (mut kl, mut kr) = (0.0, 0.0);
        let (mut ol, mut or) = (0.0, 0.0);
        let (mut dly, mut rev) = (0.0, 0.0);
        let mut key = 0.0;
        for (ch, c) in self.channels.iter_mut().enumerate() {
            let mut v = c.voice.as_impl().process(&c.real);
            c.last = v;
            if c.declick.abs() > 1e-5 {
                v += c.declick;
                c.declick *= 0.97;
            }
            if c.eq_on {
                for f in &mut c.eq {
                    v = f.process(v);
                }
            }
            for j in 0..5 {
                c.g[j] += (c.target[j] - c.g[j]) * k;
            }
            let m = v * c.g[2];
            c.meter = (c.meter * mk).max(m.abs());
            dly += v * c.g[3];
            rev += v * c.g[4];
            if ch == KICK {
                kl += v * c.g[0];
                kr += v * c.g[1];
                if let Voice::Kick(kv) = &c.voice {
                    key = kv.dry_out * c.g[2];
                }
            } else {
                ol += v * c.g[0];
                or += v * c.g[1];
            }
        }

        let (dl, dr) = self.delay.process(dly);
        let (rl, rr) = self.reverb.process(rev);
        let duck = self.ducker.process(key);
        let l = kl + (ol + dl + rl) * duck;
        let r = kr + (or + dr + rr) * duck;
        let (l, r) = self.filter.process(l, r);
        let (l, r) = self.limiter.process(l, r);
        self.vol += (self.vol_target - self.vol) * k;
        let (l, r) = (l * self.vol, r * self.vol);

        self.peak[0] = (self.peak[0] * mk).max(l.abs());
        self.peak[1] = (self.peak[1] * mk).max(r.abs());
        self.duck_gr = (self.duck_gr * mk).max(self.ducker.gr_db);
        self.lim_gr = (self.lim_gr * mk).max(self.limiter.gr_db);
        (l, r)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn command_roundtrip() {
        let cmds = [
            Command::Play,
            Command::Bpm(133.0),
            Command::SynthParam {
                ch: 3,
                idx: 2,
                norm: 0.25,
            },
            Command::Step {
                ch: 12,
                var: 2,
                step: 127,
                data: Step {
                    on: true,
                    vel: 0.5,
                    pitch: -7.25,
                    prob: 0.75,
                },
            },
            Command::TrackLen {
                ch: 1,
                var: 0,
                len: 128,
            },
            Command::QueueVar(2),
        ];
        for c in cmds {
            assert_eq!(Command::decode(&c.encode()), Some(c));
        }
        assert_eq!(
            Command::decode(&[6.0, 99.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0]),
            None
        );
    }

    #[test]
    fn status_roundtrip() {
        let mut s = Status {
            playing: true,
            var: 1,
            pending: Some(2),
            tick: 77,
            ..Default::default()
        };
        s.steps[3] = 5;
        s.meters[12] = 0.5;
        let mut a = [0.0; STATUS_LEN];
        s.write(&mut a);
        assert_eq!(Status::read(&a), Some(s));
    }

    #[test]
    fn param_mapping_roundtrip() {
        for def in &CHANNELS {
            for s in def.specs {
                let n = s.default_norm();
                let r = s.to_real(n);
                assert!(
                    (r - s.default).abs() <= 1e-3 * s.default.abs().max(1.0),
                    "{} {}",
                    def.name,
                    s.name
                );
            }
        }
    }

    fn render_secs(e: &mut Engine, secs: f32) -> (Vec<f32>, Vec<f32>) {
        let n = (e.sample_rate() * secs) as usize;
        let mut l = vec![0.0; n];
        let mut r = vec![0.0; n];
        for (cl, cr) in l.chunks_mut(128).zip(r.chunks_mut(128)) {
            e.render(cl, cr);
        }
        (l, r)
    }

    #[test]
    fn demo_renders_sane_audio() {
        let mut e = Engine::new(48000.0);
        e.load_pattern(demo_pattern());
        e.handle(Command::Play);
        let (l, r) = render_secs(&mut e, 4.0);
        let peak = l.iter().chain(&r).fold(0.0f32, |m, x| m.max(x.abs()));
        let rms = (l.iter().map(|x| x * x).sum::<f32>() / l.len() as f32).sqrt();
        assert!(l.iter().chain(&r).all(|x| x.is_finite()));
        assert!(peak <= 1.0, "peak {peak}");
        assert!(rms > 0.05, "rms {rms}");
    }

    #[test]
    fn every_channel_makes_sound_and_decays() {
        for (ch, def) in CHANNELS.iter().enumerate() {
            let mut e = Engine::new(44100.0);
            e.handle(Command::Trigger { ch, vel: 1.0 });
            let (l, _) = render_secs(&mut e, 0.3);
            let peak = l.iter().fold(0.0f32, |m, x| m.max(x.abs()));
            assert!(peak > 0.01, "{} silent: {peak}", def.name);
            let (l, _) = render_secs(&mut e, 12.0);
            let tail = l[l.len() - 4410..]
                .iter()
                .fold(0.0f32, |m, x| m.max(x.abs()));
            assert!(tail < 1e-3, "{} does not decay: {tail}", def.name);
        }
    }

    #[test]
    fn variation_switch_waits_for_loop() {
        let mut e = Engine::new(48000.0);
        let mut p = demo_pattern();
        p[KICK][0].pages = 2;
        p[KICK][0].len = 32;
        e.load_pattern(p);
        e.handle(Command::Play);
        e.handle(Command::QueueVar(1));
        let sps = e.samples_per_step();
        // 31 steps in: still on A
        render_secs(&mut e, (sps * 30.5) as f32 / 48000.0);
        assert_eq!(e.status().var, 0);
        assert_eq!(e.status().pending, Some(1));
        render_secs(&mut e, (sps * 2.0) as f32 / 48000.0);
        assert_eq!(e.status().var, 1);
        assert_eq!(e.status().pending, None);
    }

    #[test]
    fn polyrhythm_loops_independently() {
        let mut e = Engine::new(48000.0);
        let mut p = empty_pattern();
        p[3][0].len = 3;
        e.load_pattern(p);
        e.handle(Command::Play);
        let sps = e.samples_per_step();
        render_secs(&mut e, (sps * 4.5) as f32 / 48000.0);
        let s = e.status();
        assert_eq!(s.steps[3], 1); // 0,1,2,0,1
        assert_eq!(s.steps[0], 4);
    }

    #[test]
    fn mute_stops_notes_but_lets_tails_ring() {
        let mut e = Engine::new(48000.0);
        let mut p = empty_pattern();
        p[KICK][0].steps[0] = Step::hit(1.0);
        e.load_pattern(p);
        e.handle(Command::Play);
        render_secs(&mut e, 0.05);
        e.handle(Command::Mute { ch: KICK, on: true });
        // Right after muting, the kick that already hit keeps sounding.
        let (l, _) = render_secs(&mut e, 0.05);
        assert!(
            l.iter().any(|x| x.abs() > 0.05),
            "mute cut the running note"
        );
        // Past the next bar line nothing new is triggered: only a decaying tail.
        render_secs(&mut e, 3.0);
        let (l, _) = render_secs(&mut e, 2.0);
        let peak = l.iter().fold(0.0f32, |m, x| m.max(x.abs()));
        assert!(peak < 0.05, "muted kick still triggering: {peak}");
    }

    #[test]
    fn dj_filter_is_transparent_at_centre() {
        let mut f = DjFilter::new(48000.0);
        f.knob = 0.0;
        f.update();
        let mut rng = Rng::new(7);
        for _ in 0..1000 {
            let x = rng.noise();
            let (y, _) = f.process(x, x);
            assert!((x - y).abs() < 1e-5);
        }
    }
}
