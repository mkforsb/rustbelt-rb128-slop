//! Small DSP building blocks.

use std::f32::consts::{PI, TAU};

#[inline]
pub fn db_to_gain(db: f32) -> f32 {
    10f32.powf(db * 0.05)
}

#[inline]
pub fn gain_to_db(g: f32) -> f32 {
    20.0 * g.max(1e-9).log10()
}

/// Mixer-style dB → gain where the bottom of the range is silence.
#[inline]
pub fn fader_gain(db: f32) -> f32 {
    if db <= -47.9 {
        0.0
    } else {
        db_to_gain(db)
    }
}

#[inline]
pub fn semis(st: f32) -> f32 {
    (st * (1.0 / 12.0)).exp2()
}

/// Rational tanh approximation, accurate enough for saturation.
#[inline]
pub fn soft_clip(x: f32) -> f32 {
    let x = x.clamp(-3.0, 3.0);
    x * (27.0 + x * x) / (27.0 + 9.0 * x * x)
}

/// Normalized drive stage: unity gain for small signals at `drive = 0`,
/// peak-preserving for `|x| <= 1` at any drive.
#[inline]
pub fn drive(x: f32, amount: f32) -> f32 {
    let g = 1.0 + 20.0 * amount * amount;
    (g * x).tanh() / g.tanh()
}

/// Per-sample multiplier that decays to -60 dB in `ms` milliseconds.
#[inline]
pub fn decay_coef(sr: f32, ms: f32) -> f32 {
    (-6.907_755 / (ms.max(0.01) * 0.001 * sr)).exp()
}

/// One-pole smoothing coefficient for a time constant in milliseconds.
#[inline]
pub fn smooth_coef(sr: f32, ms: f32) -> f32 {
    1.0 - (-1.0 / (ms.max(0.001) * 0.001 * sr)).exp()
}

#[inline]
pub fn fast_sin(phase: f32) -> f32 {
    (phase * TAU).sin()
}

/// xorshift32: cheap, deterministic, good enough for noise and dice rolls.
#[derive(Clone, Debug)]
pub struct Rng(u32);

impl Rng {
    pub fn new(seed: u32) -> Self {
        Self(seed.max(1))
    }
    #[inline]
    pub fn next_u32(&mut self) -> u32 {
        let mut x = self.0;
        x ^= x << 13;
        x ^= x >> 17;
        x ^= x << 5;
        self.0 = x;
        x
    }
    /// Uniform in `[0, 1)`.
    #[inline]
    pub fn unit(&mut self) -> f32 {
        (self.next_u32() >> 8) as f32 * (1.0 / 16_777_216.0)
    }
    /// Uniform in `[-1, 1)`.
    #[inline]
    pub fn noise(&mut self) -> f32 {
        self.unit() * 2.0 - 1.0
    }
    pub fn range(&mut self, lo: f32, hi: f32) -> f32 {
        lo + (hi - lo) * self.unit()
    }
    pub fn below(&mut self, n: usize) -> usize {
        ((self.unit() * n as f32) as usize).min(n.saturating_sub(1))
    }
    pub fn chance(&mut self, p: f32) -> bool {
        self.unit() < p
    }
}

#[derive(Clone, Copy, Debug, Default)]
pub struct OnePole {
    a: f32,
    z: f32,
}

impl OnePole {
    pub fn set(&mut self, sr: f32, hz: f32) {
        self.a = 1.0 - (-TAU * hz.min(sr * 0.49) / sr).exp();
    }
    #[inline]
    pub fn lp(&mut self, x: f32) -> f32 {
        self.z += self.a * (x - self.z);
        self.z
    }
    #[inline]
    pub fn hp(&mut self, x: f32) -> f32 {
        x - self.lp(x)
    }
}

/// RBJ biquad, transposed direct form II.
#[derive(Clone, Copy, Debug)]
pub struct Biquad {
    b0: f32,
    b1: f32,
    b2: f32,
    a1: f32,
    a2: f32,
    z1: f32,
    z2: f32,
}

impl Default for Biquad {
    fn default() -> Self {
        Self {
            b0: 1.0,
            b1: 0.0,
            b2: 0.0,
            a1: 0.0,
            a2: 0.0,
            z1: 0.0,
            z2: 0.0,
        }
    }
}

impl Biquad {
    fn w(sr: f32, f: f32) -> (f32, f32) {
        let w = TAU * f.clamp(5.0, sr * 0.45) / sr;
        (w.cos(), w.sin())
    }

    fn norm(&mut self, b0: f32, b1: f32, b2: f32, a0: f32, a1: f32, a2: f32) {
        let i = 1.0 / a0;
        self.b0 = b0 * i;
        self.b1 = b1 * i;
        self.b2 = b2 * i;
        self.a1 = a1 * i;
        self.a2 = a2 * i;
    }

    pub fn lowpass(&mut self, sr: f32, f: f32, q: f32) {
        let (c, s) = Self::w(sr, f);
        let al = s / (2.0 * q);
        self.norm(
            (1.0 - c) / 2.0,
            1.0 - c,
            (1.0 - c) / 2.0,
            1.0 + al,
            -2.0 * c,
            1.0 - al,
        );
    }

    pub fn highpass(&mut self, sr: f32, f: f32, q: f32) {
        let (c, s) = Self::w(sr, f);
        let al = s / (2.0 * q);
        self.norm(
            (1.0 + c) / 2.0,
            -(1.0 + c),
            (1.0 + c) / 2.0,
            1.0 + al,
            -2.0 * c,
            1.0 - al,
        );
    }

    /// Band-pass with 0 dB peak gain.
    pub fn bandpass(&mut self, sr: f32, f: f32, q: f32) {
        let (c, s) = Self::w(sr, f);
        let al = s / (2.0 * q);
        self.norm(al, 0.0, -al, 1.0 + al, -2.0 * c, 1.0 - al);
    }

    pub fn peaking(&mut self, sr: f32, f: f32, q: f32, db: f32) {
        let (c, s) = Self::w(sr, f);
        let a = 10f32.powf(db / 40.0);
        let al = s / (2.0 * q);
        self.norm(
            1.0 + al * a,
            -2.0 * c,
            1.0 - al * a,
            1.0 + al / a,
            -2.0 * c,
            1.0 - al / a,
        );
    }

    pub fn lowshelf(&mut self, sr: f32, f: f32, db: f32) {
        let (c, s) = Self::w(sr, f);
        let a = 10f32.powf(db / 40.0);
        let al = s / 2.0 * 2f32.sqrt();
        let sa = 2.0 * a.sqrt() * al;
        self.norm(
            a * ((a + 1.0) - (a - 1.0) * c + sa),
            2.0 * a * ((a - 1.0) - (a + 1.0) * c),
            a * ((a + 1.0) - (a - 1.0) * c - sa),
            (a + 1.0) + (a - 1.0) * c + sa,
            -2.0 * ((a - 1.0) + (a + 1.0) * c),
            (a + 1.0) + (a - 1.0) * c - sa,
        );
    }

    pub fn highshelf(&mut self, sr: f32, f: f32, db: f32) {
        let (c, s) = Self::w(sr, f);
        let a = 10f32.powf(db / 40.0);
        let al = s / 2.0 * 2f32.sqrt();
        let sa = 2.0 * a.sqrt() * al;
        self.norm(
            a * ((a + 1.0) + (a - 1.0) * c + sa),
            -2.0 * a * ((a - 1.0) + (a + 1.0) * c),
            a * ((a + 1.0) + (a - 1.0) * c - sa),
            (a + 1.0) - (a - 1.0) * c + sa,
            2.0 * ((a - 1.0) - (a + 1.0) * c),
            (a + 1.0) - (a - 1.0) * c - sa,
        );
    }

    #[inline]
    pub fn process(&mut self, x: f32) -> f32 {
        let y = self.b0 * x + self.z1;
        self.z1 = self.b1 * x - self.a1 * y + self.z2;
        self.z2 = self.b2 * x - self.a2 * y;
        y
    }

    pub fn reset(&mut self) {
        self.z1 = 0.0;
        self.z2 = 0.0;
    }
}

/// Topology-preserving state variable filter (Zavalishin / Cytomic).
/// `low + k * band + high == input`, which the DJ filter relies on.
#[derive(Clone, Copy, Debug, Default)]
pub struct Svf {
    a1: f32,
    a2: f32,
    a3: f32,
    pub k: f32,
    ic1: f32,
    ic2: f32,
}

impl Svf {
    pub fn set(&mut self, sr: f32, f: f32, q: f32) {
        let g = (PI * f.clamp(10.0, sr * 0.45) / sr).tan();
        self.k = 1.0 / q;
        self.a1 = 1.0 / (1.0 + g * (g + self.k));
        self.a2 = g * self.a1;
        self.a3 = g * self.a2;
    }

    /// Returns `(low, band, high)`.
    #[inline]
    pub fn process(&mut self, v0: f32) -> (f32, f32, f32) {
        let v3 = v0 - self.ic2;
        let v1 = self.a1 * self.ic1 + self.a2 * v3;
        let v2 = self.ic2 + self.a2 * self.ic1 + self.a3 * v3;
        self.ic1 = 2.0 * v1 - self.ic1;
        self.ic2 = 2.0 * v2 - self.ic2;
        (v2, v1, v0 - self.k * v1 - v2)
    }
}

/// Fractional delay line with a power-of-two buffer.
#[derive(Clone, Debug)]
pub struct DelayLine {
    buf: Vec<f32>,
    mask: usize,
    pos: usize,
}

impl DelayLine {
    pub fn new(max_samples: usize) -> Self {
        let n = (max_samples + 2).next_power_of_two();
        Self {
            buf: vec![0.0; n],
            mask: n - 1,
            pos: 0,
        }
    }

    #[inline]
    pub fn write(&mut self, x: f32) {
        self.pos = (self.pos + 1) & self.mask;
        self.buf[self.pos] = x;
    }

    /// Read `d` samples behind the most recent write (`d >= 1` after writing).
    #[inline]
    pub fn read_int(&self, d: usize) -> f32 {
        self.buf[(self.pos.wrapping_sub(d)) & self.mask]
    }

    #[inline]
    pub fn read(&self, d: f32) -> f32 {
        let d = d.clamp(0.0, (self.mask - 1) as f32);
        let i = d as usize;
        let f = d - i as f32;
        let a = self.read_int(i);
        let b = self.read_int(i + 1);
        a + (b - a) * f
    }

    pub fn clear(&mut self) {
        self.buf.iter_mut().for_each(|x| *x = 0.0);
    }
}

/// Schroeder all-pass diffuser.
#[derive(Clone, Debug)]
pub struct Allpass {
    line: DelayLine,
    len: usize,
    g: f32,
}

impl Allpass {
    pub fn new(len: usize, g: f32) -> Self {
        Self {
            line: DelayLine::new(len),
            len,
            g,
        }
    }
    #[inline]
    pub fn process(&mut self, x: f32) -> f32 {
        let d = self.line.read_int(self.len - 1);
        let w = x + self.g * d;
        self.line.write(w);
        d - self.g * w
    }
}

/// Feedback delay network with a Householder matrix and per-line damping.
#[derive(Clone, Debug)]
pub struct Fdn<const N: usize> {
    lines: Vec<DelayLine>,
    lens: [usize; N],
    gains: [f32; N],
    damp: [OnePole; N],
    state: [f32; N],
    sr: f32,
}

impl<const N: usize> Fdn<N> {
    pub fn new(sr: f32, lens_ms: [f32; N]) -> Self {
        let lens = lens_ms.map(|ms| ((ms * 0.001 * sr) as usize).max(4));
        Self {
            lines: lens.iter().map(|&l| DelayLine::new(l)).collect(),
            lens,
            gains: [0.5; N],
            damp: [OnePole::default(); N],
            state: [0.0; N],
            sr,
        }
    }

    pub fn set(&mut self, rt60_s: f32, damp_hz: f32) {
        for i in 0..N {
            let len_s = self.lens[i] as f32 / self.sr;
            self.gains[i] = 10f32.powf(-3.0 * len_s / rt60_s.max(0.05));
            self.damp[i].set(self.sr, damp_hz);
        }
    }

    /// Stereo in, stereo out.
    #[inline]
    pub fn process(&mut self, l: f32, r: f32) -> (f32, f32) {
        let mut sum = 0.0;
        for i in 0..N {
            let y = self.lines[i].read_int(self.lens[i] - 1);
            let y = self.damp[i].lp(y) * self.gains[i];
            self.state[i] = y;
            sum += y;
        }
        let h = sum * (2.0 / N as f32);
        let (mut ol, mut or) = (0.0, 0.0);
        for i in 0..N {
            let y = self.state[i];
            let inp = if i % 2 == 0 { l } else { r };
            self.lines[i].write(y - h + inp);
            if i % 4 < 2 {
                ol += y;
            } else {
                or += y;
            }
            if i % 2 == 0 {
                ol += 0.3 * y;
            } else {
                or -= 0.3 * y;
            }
        }
        let s = 1.0 / (N as f32).sqrt();
        (ol * s, or * s)
    }
}

/// Shape of the DJ filter for a knob position in `[-1, 1]`.
#[derive(Clone, Copy, Debug)]
pub struct DjShape {
    /// -1 lowpass side, 0 bypass, 1 highpass side
    pub mode: i8,
    pub cutoff: f32,
    pub w_low: f32,
    pub w_band: f32,
    pub w_high: f32,
}

const DJ_DEAD: f32 = 0.015;

/// One knob, three filter responses: turning left morphs from flat through a
/// band-emphasised low-pass into a steep low-pass sweeping down; turning right
/// does the same with high-pass sweeping up. At centre the SVF outputs sum
/// back to the input exactly, so it is truly transparent.
pub fn dj_filter_shape(k: f32) -> DjShape {
    let a = k.abs();
    if a < DJ_DEAD {
        return DjShape {
            mode: 0,
            cutoff: 1000.0,
            w_low: 1.0,
            w_band: 1.0,
            w_high: 1.0,
        };
    }
    let t = ((a - DJ_DEAD) / (1.0 - DJ_DEAD)).clamp(0.0, 1.0);
    // The opposite band disappears quickly, the band-pass part lingers a bit
    // longer so the sweep passes through a band-pass flavoured response.
    let w_opp = (1.0 - t * 7.0).max(0.0);
    let w_band = (1.0 - t * 2.2).max(0.0);
    if k < 0.0 {
        let cutoff = 18000.0 * (45.0f32 / 18000.0).powf(t.powf(0.8));
        DjShape {
            mode: -1,
            cutoff,
            w_low: 1.0,
            w_band,
            w_high: w_opp,
        }
    } else {
        let cutoff = 22.0 * (9000.0f32 / 22.0).powf(t.powf(0.9));
        DjShape {
            mode: 1,
            cutoff,
            w_low: w_opp,
            w_band,
            w_high: 1.0,
        }
    }
}
