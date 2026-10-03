//! Drum and synth voices. Each voice reads real-unit parameters (already
//! mapped from knob positions) and produces a mono signal.

use crate::dsp::*;
use crate::params::*;

pub trait VoiceImpl {
    /// Called whenever a parameter changes (and once at startup).
    fn configure(&mut self, p: &[f32]);
    fn trigger(&mut self, p: &[f32], vel: f32, pitch: f32);
    fn process(&mut self, p: &[f32]) -> f32;
    /// Quick fade-out (hat choke).
    fn choke(&mut self) {}
}

// ------------------------------------------------------------------- kick

pub struct Kick {
    sr: f32,
    rng: Rng,
    active: bool,
    vel: f32,
    freq: f32,
    phase: f32,
    sub_phase: f32,
    amp: f32,
    amp_k: f32,
    sub_amp: f32,
    sub_k: f32,
    bend_env: f32,
    bend_k: f32,
    hit_env: f32,
    hit_k: f32,
    hit_bp: Biquad,
    partials: [f32; 4],
    // rumble
    rumble_fdn: Fdn<4>,
    rmb_hp: [Biquad; 2],
    rmb_lp: [Biquad; 2],
    duck_follow: f32,
    duck_rel: f32,
    /// Rumble output of the last `process` call (kept separate from the dry
    /// kick so the master ducker keys only off the dry kick).
    pub rumble_out: f32,
    pub dry_out: f32,
}

const HIT_RATIOS: [f32; 4] = [1.0, 1.593, 2.135, 2.917];

impl Kick {
    pub fn new(sr: f32) -> Self {
        Self {
            sr,
            rng: Rng::new(0x1234_5678),
            active: false,
            vel: 1.0,
            freq: 50.0,
            phase: 0.0,
            sub_phase: 0.0,
            amp: 0.0,
            amp_k: 0.0,
            sub_amp: 0.0,
            sub_k: 0.0,
            bend_env: 0.0,
            bend_k: 0.0,
            hit_env: 0.0,
            hit_k: decay_coef(sr, 18.0),
            hit_bp: Biquad::default(),
            partials: [0.0; 4],
            rumble_fdn: Fdn::new(sr, [31.7, 37.3, 41.9, 53.3]),
            rmb_hp: [Biquad::default(); 2],
            rmb_lp: [Biquad::default(); 2],
            duck_follow: 0.0,
            duck_rel: smooth_coef(sr, 60.0),
            rumble_out: 0.0,
            dry_out: 0.0,
        }
    }
}

impl VoiceImpl for Kick {
    fn configure(&mut self, p: &[f32]) {
        let sr = self.sr;
        self.amp_k = decay_coef(sr, p[K_DECAY]);
        self.sub_k = decay_coef(sr, p[K_SUB_DECAY]);
        self.bend_k = decay_coef(sr, p[K_BEND_T] * 6.9);
        self.hit_bp.bandpass(sr, p[K_HIT_TONE], 0.9);
        self.rumble_fdn.set(p[K_RMB_DECAY], 900.0);
        // Butterworth 4th order (two biquads) at each edge: a strong EQ.
        self.rmb_hp[0].highpass(sr, p[K_RMB_LO], 0.541);
        self.rmb_hp[1].highpass(sr, p[K_RMB_LO], 1.307);
        self.rmb_lp[0].lowpass(sr, p[K_RMB_HI], 0.541);
        self.rmb_lp[1].lowpass(sr, p[K_RMB_HI], 1.307);
    }

    fn trigger(&mut self, p: &[f32], vel: f32, pitch: f32) {
        self.active = true;
        self.vel = vel;
        self.freq = p[K_TUNE] * semis(pitch);
        self.phase = 0.0;
        self.sub_phase = 0.0;
        self.amp = 1.0;
        self.sub_amp = 1.0;
        self.bend_env = 1.0;
        self.hit_env = 1.0;
        self.partials = [0.0; 4];
    }

    fn process(&mut self, p: &[f32]) -> f32 {
        let sr_inv = 1.0 / self.sr;
        let mut dry = 0.0;
        let mut env = 0.0;
        if self.active {
            let f = self.freq * semis(p[K_BEND] * self.bend_env * self.bend_env);
            self.bend_env *= self.bend_k;
            self.phase = (self.phase + f * sr_inv).fract();
            let body = fast_sin(self.phase) * self.amp;

            self.sub_phase = (self.sub_phase + self.freq * sr_inv).fract();
            let sub = fast_sin(self.sub_phase) * self.sub_amp * p[K_SUB];

            // Non-tonal hit: band-passed noise plus inharmonic partials.
            let mut hit = 0.0;
            if self.hit_env > 1e-4 {
                let base = p[K_HIT_TONE] * 0.5;
                for (ph, r) in self.partials.iter_mut().zip(HIT_RATIOS) {
                    *ph = (*ph + base * r * sr_inv).fract();
                    hit += fast_sin(*ph);
                }
                hit = hit * 0.18 + self.hit_bp.process(self.rng.noise()) * 1.4;
                hit *= self.hit_env * self.hit_env * p[K_HIT];
                self.hit_env *= self.hit_k;
            }

            env = self.amp.max(self.sub_amp * p[K_SUB]);
            self.amp *= self.amp_k;
            self.sub_amp *= self.sub_k;
            if self.amp < 1e-4 && self.sub_amp < 1e-4 {
                self.active = false;
            }
            dry = drive((body * 0.8 + sub * 0.6 + hit) * self.vel, p[K_DRIVE]) * 0.9;
        }

        // Rumble: reverb fed by the kick, driven, heavily band-limited and
        // ducked by the kick's own envelope so it swells between hits.
        let send = dry * p[K_RMB];
        let (wet, _) = self.rumble_fdn.process(send, send);
        let mut r = drive(wet * 1.5, p[K_RMB_DRIVE]);
        for f in &mut self.rmb_hp {
            r = f.process(r);
        }
        for f in &mut self.rmb_lp {
            r = f.process(r);
        }
        let e = env * self.vel;
        if e > self.duck_follow {
            self.duck_follow = e;
        } else {
            self.duck_follow += (e - self.duck_follow) * self.duck_rel;
        }
        let duck = (1.0 - p[K_RMB_DUCK] * (self.duck_follow * 1.6).min(1.0)).max(0.0);
        self.rumble_out = r * duck * duck * 2.2;
        self.dry_out = dry;
        dry + self.rumble_out
    }
}

// ------------------------------------------------------- metallic (hats)

const HAT_RATIOS: [f32; 6] = [1.0, 1.4471, 1.617, 1.9265, 2.5028, 2.6637];
const RIDE_RATIOS: [f32; 6] = [1.0, 1.341, 1.783, 2.411, 2.738, 3.912];

pub struct Metal {
    sr: f32,
    ride: bool,
    rng: Rng,
    phases: [f32; 6],
    base: f32,
    vel: f32,
    att_pos: f32,
    att_inc: f32,
    env: f32,
    dec_k: f32,
    choke_k: f32,
    choking: bool,
    bell_env: f32,
    bell_k: f32,
    bp: Biquad,
    hp: Biquad,
    hp2: Biquad,
    bell_bp: Biquad,
}

impl Metal {
    pub fn new(sr: f32, ride: bool, seed: u32) -> Self {
        Self {
            sr,
            ride,
            rng: Rng::new(seed),
            phases: [0.0; 6],
            base: 400.0,
            vel: 0.0,
            att_pos: 1.0,
            att_inc: 1.0,
            env: 0.0,
            dec_k: 0.0,
            choke_k: decay_coef(sr, 12.0),
            choking: false,
            bell_env: 0.0,
            bell_k: 0.0,
            bp: Biquad::default(),
            hp: Biquad::default(),
            hp2: Biquad::default(),
            bell_bp: Biquad::default(),
        }
    }
}

impl VoiceImpl for Metal {
    fn configure(&mut self, p: &[f32]) {
        let sr = self.sr;
        self.dec_k = decay_coef(sr, p[M_DECAY]);
        self.att_inc = 1.0 / (p[M_ATTACK] * 0.001 * sr).max(1.0);
        let tone = p[M_TONE];
        if self.ride {
            self.bp.bandpass(sr, tone, 0.7);
            self.hp.highpass(sr, tone * 0.45, 0.707);
            self.hp2.highpass(sr, 900.0, 0.707);
            self.bell_bp.bandpass(sr, 2600.0 * semis(p[M_TUNE]), 2.5);
            self.bell_k = decay_coef(sr, p[M_DECAY] * 0.6);
        } else {
            self.bp.bandpass(sr, tone * 1.15, 0.9);
            self.hp.highpass(sr, tone * 0.8, 0.707);
            self.hp2.highpass(sr, tone * 0.8, 0.707);
        }
    }

    fn trigger(&mut self, p: &[f32], vel: f32, pitch: f32) {
        let base = if self.ride { 385.0 } else { 410.0 };
        self.base = base * semis(p[M_TUNE] + pitch);
        self.vel = vel;
        self.att_pos = 0.0;
        self.env = 1.0;
        self.bell_env = 1.0;
        self.choking = false;
    }

    fn choke(&mut self) {
        self.choking = true;
    }

    fn process(&mut self, p: &[f32]) -> f32 {
        if self.env < 1e-5 {
            return 0.0;
        }
        let ratios = if self.ride { &RIDE_RATIOS } else { &HAT_RATIOS };
        let inv = 1.0 / self.sr;
        let mut sq = 0.0;
        let mut bell = 0.0;
        for (i, (ph, r)) in self.phases.iter_mut().zip(ratios).enumerate() {
            *ph = (*ph + self.base * r * inv).fract();
            let s = if *ph < 0.5 { 1.0 } else { -1.0 };
            sq += s;
            if i < 3 {
                bell += s;
            }
        }
        sq *= 1.0 / 6.0;
        let noise = self.rng.noise();
        let mix = sq * (1.0 - p[M_NOISE]) + noise * p[M_NOISE];
        let mut y = self.hp2.process(self.hp.process(self.bp.process(mix)));
        let att = if self.att_pos < 1.0 {
            self.att_pos += self.att_inc;
            self.att_pos.min(1.0)
        } else {
            1.0
        };
        y *= self.env * att * 3.2;
        if self.ride {
            let b = self.bell_bp.process(bell * (1.0 / 3.0)) * self.bell_env * p[M_BELL] * 2.0;
            self.bell_env *= self.bell_k;
            y += b * att;
        }
        self.env *= if self.choking {
            self.choke_k
        } else {
            self.dec_k
        };
        y * self.vel
    }
}

// ------------------------------------------------------------------ snare

pub struct Snare {
    sr: f32,
    rng: Rng,
    freq: f32,
    ph: [f32; 2],
    vel: f32,
    amp: f32,
    amp_k: f32,
    namp: f32,
    namp_k: f32,
    bend: f32,
    bend_k: f32,
    hp: Biquad,
    bp: Biquad,
}

impl Snare {
    pub fn new(sr: f32) -> Self {
        Self {
            sr,
            rng: Rng::new(0xBEEF_1234),
            freq: 180.0,
            ph: [0.0; 2],
            vel: 0.0,
            amp: 0.0,
            amp_k: 0.0,
            namp: 0.0,
            namp_k: 0.0,
            bend: 0.0,
            bend_k: decay_coef(sr, 25.0),
            hp: Biquad::default(),
            bp: Biquad::default(),
        }
    }
}

impl VoiceImpl for Snare {
    fn configure(&mut self, p: &[f32]) {
        self.amp_k = decay_coef(self.sr, p[S_DECAY]);
        self.namp_k = decay_coef(self.sr, p[S_NDECAY]);
        self.hp.highpass(self.sr, p[S_NTONE] * 0.35, 0.707);
        self.bp.peaking(self.sr, p[S_NTONE], 0.8, 6.0);
    }

    fn trigger(&mut self, p: &[f32], vel: f32, pitch: f32) {
        self.freq = p[S_TUNE] * semis(pitch);
        self.ph = [0.0, 0.25];
        self.vel = vel;
        self.amp = 1.0;
        self.namp = 1.0;
        self.bend = 1.0;
    }

    fn process(&mut self, p: &[f32]) -> f32 {
        if self.amp < 1e-5 && self.namp < 1e-5 {
            return 0.0;
        }
        let inv = 1.0 / self.sr;
        let f = self.freq * semis(p[S_BEND] * self.bend);
        self.bend *= self.bend_k;
        self.ph[0] = (self.ph[0] + f * inv).fract();
        self.ph[1] = (self.ph[1] + f * 1.618 * inv).fract();
        let body = (fast_sin(self.ph[0]) * 0.65 + fast_sin(self.ph[1]) * 0.35) * self.amp;
        let n = self.bp.process(self.hp.process(self.rng.noise())) * self.namp * p[S_SNAPPY];
        self.amp *= self.amp_k;
        self.namp *= self.namp_k;
        soft_clip((body * (1.0 - 0.4 * p[S_SNAPPY]) + n * 1.1) * self.vel)
    }
}

// ------------------------------------------------------------------- clap

pub struct Clap {
    sr: f32,
    rng: Rng,
    bp: Biquad,
    hp: Biquad,
    vel: f32,
    env: f32,
    t: f32,
    next_burst: f32,
    bursts_left: u32,
    burst_k: f32,
    tail_k: f32,
    spread: f32,
}

impl Clap {
    pub fn new(sr: f32) -> Self {
        Self {
            sr,
            rng: Rng::new(0xC1A9_0001),
            bp: Biquad::default(),
            hp: Biquad::default(),
            vel: 0.0,
            env: 0.0,
            t: 0.0,
            next_burst: 0.0,
            bursts_left: 0,
            burst_k: 0.0,
            tail_k: 0.0,
            spread: 0.0,
        }
    }
}

impl VoiceImpl for Clap {
    fn configure(&mut self, p: &[f32]) {
        self.bp.bandpass(self.sr, p[C_TONE], p[C_Q]);
        self.hp.highpass(self.sr, 350.0, 0.707);
        self.spread = p[C_SPREAD] * 0.001 * self.sr;
        self.burst_k = decay_coef(self.sr, p[C_SPREAD] * 1.6);
        self.tail_k = decay_coef(self.sr, p[C_DECAY]);
    }

    fn trigger(&mut self, p: &[f32], vel: f32, _pitch: f32) {
        self.vel = vel;
        self.env = 1.0;
        self.t = 0.0;
        self.bursts_left = p[C_BURSTS] as u32;
        self.next_burst = self.spread * (0.85 + 0.3 * self.rng.unit());
    }

    fn process(&mut self, _p: &[f32]) -> f32 {
        if self.env < 1e-5 && self.bursts_left == 0 {
            return 0.0;
        }
        self.t += 1.0;
        if self.bursts_left > 0 && self.t >= self.next_burst {
            self.bursts_left -= 1;
            self.env = 1.0;
            self.next_burst += self.spread * (0.85 + 0.3 * self.rng.unit());
        }
        let y = self.hp.process(self.bp.process(self.rng.noise()));
        let out = y * self.env * self.vel * 3.0;
        self.env *= if self.bursts_left > 0 {
            self.burst_k
        } else {
            self.tail_k
        };
        soft_clip(out)
    }
}

// ----------------------------------------------------- toms and congas

pub struct Perc {
    sr: f32,
    conga: bool,
    rng: Rng,
    freq: f32,
    phase: f32,
    vel: f32,
    amp: f32,
    amp_k: f32,
    bend: f32,
    bend_k: f32,
    nenv: f32,
    nenv_k: f32,
    nf: Biquad,
}

impl Perc {
    pub fn new(sr: f32, conga: bool, seed: u32) -> Self {
        Self {
            sr,
            conga,
            rng: Rng::new(seed),
            freq: 100.0,
            phase: 0.0,
            vel: 0.0,
            amp: 0.0,
            amp_k: 0.0,
            bend: 0.0,
            bend_k: 0.0,
            nenv: 0.0,
            nenv_k: decay_coef(sr, if conga { 12.0 } else { 40.0 }),
            nf: Biquad::default(),
        }
    }
}

impl VoiceImpl for Perc {
    fn configure(&mut self, p: &[f32]) {
        self.amp_k = decay_coef(self.sr, p[T_DECAY]);
        self.bend_k = decay_coef(self.sr, p[T_BEND_T] * 6.9);
        if self.conga {
            self.nf
                .bandpass(self.sr, (p[T_TUNE] * 7.0).min(9000.0), 1.4);
        } else {
            self.nf
                .bandpass(self.sr, (p[T_TUNE] * 10.0).min(8000.0), 0.6);
        }
    }

    fn trigger(&mut self, p: &[f32], vel: f32, pitch: f32) {
        self.freq = p[T_TUNE] * semis(pitch);
        self.phase = 0.0;
        self.vel = vel;
        self.amp = 1.0;
        self.bend = 1.0;
        self.nenv = 1.0;
    }

    fn process(&mut self, p: &[f32]) -> f32 {
        if self.amp < 1e-5 {
            return 0.0;
        }
        let f = self.freq * semis(p[T_BEND] * self.bend);
        self.bend *= self.bend_k;
        self.phase = (self.phase + f / self.sr).fract();
        let body = fast_sin(self.phase) * self.amp;
        let n = self.nf.process(self.rng.noise()) * self.nenv * p[T_NOISE] * 2.5;
        self.amp *= self.amp_k;
        self.nenv *= self.nenv_k;
        soft_clip((body + n) * self.vel * 1.1)
    }
}

// ------------------------------------------------------------- FM stab

pub struct Stab {
    sr: f32,
    freqs: [f32; 3],
    cph: [f32; 3],
    mph: [f32; 3],
    fb: [f32; 3],
    vel: f32,
    att_pos: f32,
    att_inc: f32,
    amp: f32,
    amp_k: f32,
    idx: f32,
    idx_k: f32,
    lp: Svf,
    lp2: Svf,
}

impl Stab {
    pub fn new(sr: f32) -> Self {
        Self {
            sr,
            freqs: [0.0; 3],
            cph: [0.0; 3],
            mph: [0.0; 3],
            fb: [0.0; 3],
            vel: 0.0,
            att_pos: 1.0,
            att_inc: 1.0,
            amp: 0.0,
            amp_k: 0.0,
            idx: 0.0,
            idx_k: 0.0,
            lp: Svf::default(),
            lp2: Svf::default(),
        }
    }
}

impl VoiceImpl for Stab {
    fn configure(&mut self, p: &[f32]) {
        self.amp_k = decay_coef(self.sr, p[F_DECAY]);
        self.idx_k = decay_coef(self.sr, p[F_MOD_DEC]);
        self.att_inc = 1.0 / (p[F_ATTACK] * 0.001 * self.sr).max(1.0);
        self.lp.set(self.sr, p[F_CUTOFF], 0.8);
        self.lp2.set(self.sr, p[F_CUTOFF], 0.6);
    }

    fn trigger(&mut self, p: &[f32], vel: f32, pitch: f32) {
        // Free voice pitches around C3 (130.81 Hz), shifted by Tune and the
        // per-step pitch.
        let base = p[F_TUNE] + pitch;
        let det = p[F_DETUNE] / 100.0;
        let voices = [p[F_PITCH1], p[F_PITCH2], p[F_PITCH3]];
        for (i, (f, v)) in self.freqs.iter_mut().zip(voices).enumerate() {
            *f = 130.813 * semis(base + v + (i as f32 - 1.0) * det);
        }
        self.vel = vel;
        self.att_pos = 0.0;
        self.amp = 1.0;
        self.idx = p[F_INDEX] * (0.4 + 0.6 * vel);
    }

    fn process(&mut self, p: &[f32]) -> f32 {
        if self.amp < 1e-5 {
            return 0.0;
        }
        let inv = 1.0 / self.sr;
        let ratio = RATIOS[(p[F_RATIO] as usize).min(RATIOS.len() - 1)];
        let fbk = p[F_FEEDBACK] * 1.5;
        let mut sum = 0.0;
        for i in 0..3 {
            self.mph[i] = (self.mph[i] + self.freqs[i] * ratio * inv).fract();
            self.cph[i] = (self.cph[i] + self.freqs[i] * inv).fract();
            let m = (self.mph[i] * std::f32::consts::TAU + fbk * self.fb[i]).sin();
            self.fb[i] = 0.5 * (self.fb[i] + m);
            sum += (self.cph[i] * std::f32::consts::TAU + self.idx * m).sin();
        }
        self.idx *= self.idx_k;
        let att = if self.att_pos < 1.0 {
            self.att_pos += self.att_inc;
            self.att_pos.min(1.0)
        } else {
            1.0
        };
        let y = sum * (1.0 / 3.0) * self.amp * att;
        self.amp *= self.amp_k;
        let (l, _, _) = self.lp.process(y);
        let (l, _, _) = self.lp2.process(l);
        soft_clip(l * self.vel * 1.4)
    }
}

pub enum Voice {
    Kick(Box<Kick>),
    Metal(Metal),
    Snare(Snare),
    Clap(Clap),
    Perc(Perc),
    Stab(Stab),
}

impl Voice {
    pub fn new(kind: Kind, sr: f32, ch: usize) -> Self {
        let seed = 0x9E37_79B9u32.wrapping_mul(ch as u32 + 1);
        match kind {
            Kind::Kick => Voice::Kick(Box::new(Kick::new(sr))),
            Kind::Hat => Voice::Metal(Metal::new(sr, false, seed)),
            Kind::Ride => Voice::Metal(Metal::new(sr, true, seed)),
            Kind::Snare => Voice::Snare(Snare::new(sr)),
            Kind::Clap => Voice::Clap(Clap::new(sr)),
            Kind::Perc => Voice::Perc(Perc::new(sr, ch >= 9, seed)),
            Kind::Stab => Voice::Stab(Stab::new(sr)),
        }
    }

    pub fn as_impl(&mut self) -> &mut dyn VoiceImpl {
        match self {
            Voice::Kick(v) => v.as_mut(),
            Voice::Metal(v) => v,
            Voice::Snare(v) => v,
            Voice::Clap(v) => v,
            Voice::Perc(v) => v,
            Voice::Stab(v) => v,
        }
    }
}
