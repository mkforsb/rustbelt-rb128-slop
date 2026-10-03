//! Master effects: tempo-synced delay, reverb, kick-keyed ducker, DJ filter
//! and look-ahead limiter.

use crate::dsp::*;

pub struct Delay {
    sr: f32,
    l: DelayLine,
    r: DelayLine,
    time: f32,
    target: f32,
    pub feedback: f32,
    pub width: f32,
    pub level: f32,
    lp: [OnePole; 2],
    hp: [OnePole; 2],
}

impl Delay {
    pub fn new(sr: f32) -> Self {
        let max = (sr * 4.2) as usize;
        Self {
            sr,
            l: DelayLine::new(max),
            r: DelayLine::new(max),
            time: sr * 0.25,
            target: sr * 0.25,
            feedback: 0.4,
            width: 0.8,
            level: 0.8,
            lp: [OnePole::default(); 2],
            hp: [OnePole::default(); 2],
        }
    }

    pub fn set_time_samples(&mut self, t: f32) {
        self.target = t.clamp(16.0, self.sr * 4.0);
    }

    pub fn set_filters(&mut self, hicut: f32, locut: f32) {
        for f in &mut self.lp {
            f.set(self.sr, hicut);
        }
        for f in &mut self.hp {
            f.set(self.sr, locut);
        }
    }

    #[inline]
    pub fn process(&mut self, x: f32) -> (f32, f32) {
        // Glide towards a new time (tape-style) rather than jumping.
        self.time += (self.target - self.time) * 0.0007;
        let dl = self.l.read(self.time - 1.0);
        let dr = self.r.read(self.time - 1.0);
        let fl = self.hp[0].hp(self.lp[0].lp(dl));
        let fr = self.hp[1].hp(self.lp[1].lp(dr));
        let w = self.width;
        let fb = self.feedback;
        let in_l = x + fb * (w * fr + (1.0 - w) * fl);
        let in_r = x * (1.0 - w) + fb * (w * fl + (1.0 - w) * fr);
        self.l.write(soft_clip(in_l));
        self.r.write(soft_clip(in_r));
        (dl * self.level, dr * self.level)
    }
}

pub struct Reverb {
    sr: f32,
    pre: DelayLine,
    pre_len: f32,
    hp: OnePole,
    diff: [Allpass; 4],
    fdn: Fdn<8>,
    pub level: f32,
}

impl Reverb {
    pub fn new(sr: f32) -> Self {
        let ms = |m: f32| (m * 0.001 * sr) as usize;
        Self {
            sr,
            pre: DelayLine::new((sr * 0.25) as usize),
            pre_len: 0.0,
            hp: OnePole::default(),
            diff: [
                Allpass::new(ms(4.77), 0.7),
                Allpass::new(ms(3.59), 0.7),
                Allpass::new(ms(12.73), 0.6),
                Allpass::new(ms(9.31), 0.6),
            ],
            fdn: Fdn::new(sr, [29.7, 37.1, 41.1, 43.7, 53.3, 59.9, 67.7, 73.1]),
            level: 0.8,
        }
    }

    pub fn set(&mut self, decay_s: f32, damp: f32, pre_ms: f32, locut: f32) {
        self.fdn.set(decay_s, damp);
        self.pre_len = pre_ms * 0.001 * self.sr;
        self.hp.set(self.sr, locut);
    }

    #[inline]
    pub fn process(&mut self, x: f32) -> (f32, f32) {
        self.pre.write(x);
        let mut d = self.hp.hp(self.pre.read(self.pre_len));
        for a in &mut self.diff {
            d = a.process(d);
        }
        let (l, r) = self.fdn.process(d, -d);
        (l * self.level, r * self.level)
    }
}

/// Sidechain compressor keyed by the kick.
pub struct Ducker {
    sr: f32,
    env: f32,
    att: f32,
    rel: f32,
    pub thresh: f32,
    slope: f32,
    pub gr_db: f32,
}

impl Ducker {
    pub fn new(sr: f32) -> Self {
        Self {
            sr,
            env: 0.0,
            att: 0.5,
            rel: 0.001,
            thresh: -20.0,
            slope: 0.75,
            gr_db: 0.0,
        }
    }

    pub fn set(&mut self, thresh: f32, ratio: f32, att_ms: f32, rel_ms: f32) {
        self.thresh = thresh;
        self.slope = 1.0 - 1.0 / ratio.max(1.0);
        self.att = smooth_coef(self.sr, att_ms);
        self.rel = smooth_coef(self.sr, rel_ms);
    }

    /// Returns the gain to apply to the ducked bus.
    #[inline]
    pub fn process(&mut self, key: f32) -> f32 {
        let a = key.abs();
        let c = if a > self.env { self.att } else { self.rel };
        self.env += (a - self.env) * c;
        let over = gain_to_db(self.env) - self.thresh;
        if over > 0.0 {
            let gr = over * self.slope;
            self.gr_db = gr;
            db_to_gain(-gr)
        } else {
            self.gr_db = 0.0;
            1.0
        }
    }
}

pub struct DjFilter {
    sr: f32,
    svf: [Svf; 2],
    pub knob: f32,
    smoothed: f32,
    res: f32,
    shape: DjShape,
}

impl DjFilter {
    pub fn new(sr: f32) -> Self {
        let mut f = Self {
            sr,
            svf: [Svf::default(); 2],
            knob: 0.0,
            smoothed: 0.0,
            res: 0.3,
            shape: dj_filter_shape(0.0),
        };
        f.update();
        f
    }

    pub fn set_res(&mut self, r: f32) {
        self.res = r;
    }

    /// Call once per control block.
    pub fn update(&mut self) {
        self.smoothed += (self.knob - self.smoothed) * 0.25;
        self.shape = dj_filter_shape(self.smoothed);
        // Resonance builds as the filter closes, scaled by the Reso knob.
        let t = self.smoothed.abs();
        let q = 0.707 + self.res * 6.0 * t.min(0.6) / 0.6;
        for s in &mut self.svf {
            s.set(self.sr, self.shape.cutoff, q);
        }
    }

    #[inline]
    pub fn process(&mut self, l: f32, r: f32) -> (f32, f32) {
        if self.shape.mode == 0 && self.smoothed.abs() < 1e-3 {
            // keep state warm so engaging the filter is click-free
            self.svf[0].process(l);
            self.svf[1].process(r);
            return (l, r);
        }
        let s = self.shape;
        let mut out = [0.0; 2];
        for (i, x) in [l, r].into_iter().enumerate() {
            let k = self.svf[i].k;
            let (lo, bp, hi) = self.svf[i].process(x);
            out[i] = s.w_low * lo + s.w_band * k * bp + s.w_high * hi;
        }
        (out[0], out[1])
    }
}

const LOOKAHEAD: usize = 64;

pub struct Limiter {
    sr: f32,
    dl: DelayLine,
    dr: DelayLine,
    targets: [f32; LOOKAHEAD],
    tpos: usize,
    gain: f32,
    att: f32,
    rel: f32,
    pub makeup: f32,
    pub ceiling: f32,
    pub gr_db: f32,
}

impl Limiter {
    pub fn new(sr: f32) -> Self {
        Self {
            sr,
            dl: DelayLine::new(LOOKAHEAD + 4),
            dr: DelayLine::new(LOOKAHEAD + 4),
            targets: [1.0; LOOKAHEAD],
            tpos: 0,
            gain: 1.0,
            att: 1.0 - (-4.0 / LOOKAHEAD as f32).exp(),
            rel: 0.001,
            makeup: 1.0,
            ceiling: 0.97,
            gr_db: 0.0,
        }
    }

    pub fn set(&mut self, makeup_db: f32, ceil_db: f32, rel_ms: f32) {
        self.makeup = db_to_gain(makeup_db);
        self.ceiling = db_to_gain(ceil_db);
        self.rel = smooth_coef(self.sr, rel_ms);
    }

    #[inline]
    pub fn process(&mut self, l: f32, r: f32) -> (f32, f32) {
        let (l, r) = (l * self.makeup, r * self.makeup);
        let peak = l.abs().max(r.abs());
        let t = if peak > self.ceiling {
            self.ceiling / peak
        } else {
            1.0
        };
        self.targets[self.tpos] = t;
        self.tpos = (self.tpos + 1) % LOOKAHEAD;
        let mut m = 1.0f32;
        for &v in &self.targets {
            m = m.min(v);
        }
        let c = if m < self.gain { self.att } else { self.rel };
        self.gain += (m - self.gain) * c;
        self.gr_db = -gain_to_db(self.gain);
        self.dl.write(l);
        self.dr.write(r);
        let ol = self.dl.read_int(LOOKAHEAD - 1) * self.gain;
        let or = self.dr.read_int(LOOKAHEAD - 1) * self.gain;
        let c = self.ceiling;
        (ol.clamp(-c, c), or.clamp(-c, c))
    }
}
