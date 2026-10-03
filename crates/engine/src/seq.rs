//! Pattern data model, shared by the UI (editing) and the engine (playback).

use crate::dsp::Rng;
use crate::params::*;

pub const MAX_STEPS: usize = 128;
pub const PAGE: usize = 16;
pub const MAX_PAGES: usize = MAX_STEPS / PAGE;
pub const NUM_VARS: usize = 3;
/// Per-step pitch range in (free, non-quantized) semitones: ±2 octaves.
pub const PITCH_RANGE: f32 = 24.0;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Step {
    pub on: bool,
    pub vel: f32,
    /// Free tuning offset in semitones, `-PITCH_RANGE..=PITCH_RANGE`.
    pub pitch: f32,
    pub prob: f32,
}

impl Default for Step {
    fn default() -> Self {
        Self {
            on: false,
            vel: 0.8,
            pitch: 0.0,
            prob: 1.0,
        }
    }
}

impl Step {
    pub fn hit(vel: f32) -> Self {
        Self {
            on: true,
            vel,
            ..Self::default()
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Track {
    pub steps: Vec<Step>,
    /// Loop length in steps, `1..=128`. Anything shorter than `pages * 16`
    /// is a polyrhythmic loop point.
    pub len: usize,
    /// Number of 16-step pages in use (1..=8).
    pub pages: usize,
}

impl Default for Track {
    fn default() -> Self {
        Self {
            steps: vec![Step::default(); MAX_STEPS],
            len: PAGE,
            pages: 1,
        }
    }
}

impl Track {
    pub fn set_pages(&mut self, pages: usize) {
        let pages = pages.clamp(1, MAX_PAGES);
        // A loop point at the end of the old length follows the new end.
        if self.len == self.pages * PAGE || self.len > pages * PAGE {
            self.len = pages * PAGE;
        }
        self.pages = pages;
    }

    /// MC-style "copy and extend": doubles the pattern by duplicating it.
    pub fn duplicate(&mut self) {
        let n = self.pages * PAGE;
        if n * 2 > MAX_STEPS {
            return;
        }
        let looped = self.len;
        for i in 0..n {
            self.steps[n + i] = self.steps[i % looped.max(1)];
        }
        self.pages *= 2;
        self.len = self.pages * PAGE;
    }
}

pub type Pattern = Vec<[Track; NUM_VARS]>;

pub fn empty_pattern() -> Pattern {
    (0..NUM_CH).map(|_| Default::default()).collect()
}

fn set(t: &mut Track, idx: &[usize], vel: f32) {
    for &i in idx {
        t.steps[i] = Step::hit(vel);
    }
}

/// A small demo groove so the box makes noise as soon as you press play.
pub fn demo_pattern() -> Pattern {
    let mut p = empty_pattern();
    // Variation A: driving four-on-the-floor.
    set(&mut p[KICK][0], &[0, 4, 8, 12], 1.0);
    let t = &mut p[CLOSED_HAT][0];
    set(t, &[0, 1, 3, 4, 5, 7, 8, 9, 11, 12, 13, 15], 0.55);
    for i in [1, 5, 9, 13] {
        t.steps[i].vel = 0.35;
        t.steps[i].prob = 0.7;
    }
    set(&mut p[OPEN_HAT][0], &[2, 6, 10, 14], 0.7);
    set(&mut p[5][0], &[4, 12], 0.85);
    // 3-step polyrhythmic ride loop
    let t = &mut p[3][0];
    set(t, &[0, 2], 0.45);
    t.len = 3;
    let t = &mut p[10][0];
    set(t, &[3, 7, 11, 14], 0.6);
    t.steps[14].pitch = 2.0;
    t.steps[11].prob = 0.5;
    let t = &mut p[STAB][0];
    set(t, &[3, 10], 0.8);
    t.steps[10].pitch = -2.0;
    t.steps[10].vel = 0.6;
    // Variation B: A plus more energy.
    for vars in p.iter_mut() {
        vars[1] = vars[0].clone();
    }
    set(&mut p[4][1], &[4, 12, 15], 0.75);
    p[4][1].steps[15].vel = 0.4;
    set(&mut p[9][1], &[2, 5, 8, 13], 0.6);
    set(&mut p[STAB][1], &[6, 14], 0.55);
    // Variation C: breakdown, no kick, toms
    for vars in p.iter_mut().skip(1) {
        vars[2] = vars[0].clone();
    }
    p[OPEN_HAT][2] = Track::default();
    set(&mut p[6][2], &[0, 6, 12], 0.7);
    set(&mut p[7][2], &[3, 9], 0.6);
    set(&mut p[8][2], &[14], 0.6);
    p
}

/// Per-instrument step weights for musically-flavoured randomization.
fn weights(ch: usize, i: usize) -> f32 {
    let q = i % 4;
    match ch {
        KICK => [0.95, 0.04, 0.1, 0.12][q],
        CLOSED_HAT => [0.55, 0.4, 0.75, 0.45][q],
        OPEN_HAT => [0.03, 0.05, 0.85, 0.08][q],
        3 => [0.5, 0.35, 0.6, 0.35][q],
        4 | 5 => {
            if i % 8 == 4 {
                0.85
            } else {
                [0.02, 0.08, 0.06, 0.12][q]
            }
        }
        6..=11 => [0.12, 0.22, 0.18, 0.3][q],
        _ => [0.12, 0.25, 0.3, 0.35][q],
    }
}

/// Fill `steps[range]` with a fresh random rhythm for a channel.
pub fn randomize_steps(
    t: &mut Track,
    ch: usize,
    density: f32,
    range: std::ops::Range<usize>,
    rng: &mut Rng,
) {
    for i in range {
        let w = (weights(ch, i) * density * 2.0).clamp(0.0, 0.98);
        let on = rng.chance(w);
        let s = &mut t.steps[i];
        s.on = on;
        if on {
            s.vel = rng.range(0.55, 1.0);
            if ch == KICK && i % 4 == 0 {
                s.vel = 1.0;
            }
        }
    }
}

pub fn randomize_vel(t: &mut Track, range: std::ops::Range<usize>, rng: &mut Rng) {
    for i in range {
        t.steps[i].vel = rng.range(0.3, 1.0);
    }
}

pub fn randomize_pitch(t: &mut Track, ch: usize, range: std::ops::Range<usize>, rng: &mut Rng) {
    // Free tuning: stabs move around more, drums wobble a little.
    let (chance, spread) = if ch == STAB { (0.6, 7.0) } else { (0.35, 5.0) };
    for i in range {
        t.steps[i].pitch = if rng.chance(chance) {
            rng.range(-spread, spread)
        } else {
            0.0
        };
    }
}

pub fn randomize_prob(t: &mut Track, range: std::ops::Range<usize>, rng: &mut Rng) {
    for i in range {
        t.steps[i].prob = if rng.chance(0.6) {
            1.0
        } else {
            rng.range(0.25, 0.9)
        };
    }
}

/// Flip a few steps: the "keep it evolving" button.
pub fn mutate(t: &mut Track, ch: usize, range: std::ops::Range<usize>, rng: &mut Rng) {
    let n = range.len();
    let flips = 1 + n / 12;
    for _ in 0..flips {
        let i = range.start + rng.below(n);
        let s = &mut t.steps[i];
        if s.on {
            s.on = rng.chance(0.3) && weights(ch, i) > 0.5;
        } else if rng.chance(weights(ch, i) + 0.25) {
            *s = Step::hit(rng.range(0.5, 1.0));
        }
    }
}
