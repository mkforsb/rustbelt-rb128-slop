//! UI-side model. The UI owns the source of truth and mirrors every change to
//! the engine as a `Command`.

use crate::audio;
use dioxus::prelude::*;
use rustbelt_engine::{dsp::Rng, params::*, seq::*, Command, Status};
use std::cell::RefCell;
use std::ops::Range;

#[derive(Clone, PartialEq)]
pub struct ChannelState {
    pub synth: Vec<f32>,
    pub mix: [f32; NUM_MIX],
    pub mute: bool,
    pub solo: bool,
}

impl ChannelState {
    fn new(ch: usize) -> Self {
        let mut mix = [0.0; NUM_MIX];
        for (m, s) in mix.iter_mut().zip(&MIX_SPECS) {
            *m = s.default_norm();
        }
        // a starting mix that sounds like a track rather than a test tone
        let set =
            |mix: &mut [f32; NUM_MIX], i: usize, real: f32| mix[i] = MIX_SPECS[i].to_norm(real);
        match ch {
            KICK => set(&mut mix, MIX_LEVEL, -3.0),
            1 => {
                set(&mut mix, MIX_PAN, 0.12);
                set(&mut mix, MIX_LEVEL, -9.0)
            }
            2 => {
                set(&mut mix, MIX_PAN, -0.1);
                set(&mut mix, MIX_LEVEL, -10.0);
                set(&mut mix, MIX_REV, 0.15)
            }
            3 => {
                set(&mut mix, MIX_PAN, 0.35);
                set(&mut mix, MIX_LEVEL, -12.0);
                set(&mut mix, MIX_DLY, 0.2)
            }
            5 => {
                set(&mut mix, MIX_LEVEL, -6.0);
                set(&mut mix, MIX_REV, 0.35)
            }
            9..=11 => {
                set(&mut mix, MIX_PAN, [-0.4, 0.3, 0.55][ch - 9]);
                set(&mut mix, MIX_DLY, 0.25);
                set(&mut mix, MIX_LEVEL, -8.0)
            }
            STAB => {
                set(&mut mix, MIX_LEVEL, -9.0);
                set(&mut mix, MIX_DLY, 0.45);
                set(&mut mix, MIX_REV, 0.3)
            }
            _ => {}
        }
        Self {
            synth: CHANNELS[ch]
                .specs
                .iter()
                .map(|s| s.default_norm())
                .collect(),
            mix,
            mute: false,
            solo: false,
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Lane {
    Vel,
    Pitch,
    Prob,
}

#[derive(Clone, PartialEq)]
pub struct Ui {
    pub ch: usize,
    /// Variation being edited (the last one the user picked).
    pub var: usize,
    pub page: usize,
    pub lane: Lane,
    pub loop_mode: bool,
    pub follow: bool,
}

#[derive(Clone, Copy, PartialEq)]
pub struct Transport {
    pub bpm: f32,
    pub swing: f32,
}

#[derive(Clone, PartialEq)]
pub enum Clip {
    Page(Vec<Step>),
    Track(Track),
}

#[derive(Clone, Copy, PartialEq)]
pub struct KnobDrag {
    pub start_y: f64,
    pub start: f32,
    pub onchange: Callback<f32>,
    /// Normalized quantum (0 = continuous).
    pub step: f32,
}

#[derive(Clone, Copy, PartialEq, Default)]
pub struct PlayStatus {
    pub playing: bool,
    pub var: usize,
    pub pending: Option<usize>,
    pub steps: [i32; NUM_CH],
}

#[derive(Clone, Copy, PartialEq, Default)]
pub struct Meters {
    pub ch: [f32; NUM_CH],
    pub peak: [f32; 2],
    pub duck_gr: f32,
    pub lim_gr: f32,
}

thread_local! {
    static RNG: RefCell<Rng> = RefCell::new(Rng::new(seed()));
}

fn seed() -> u32 {
    #[cfg(target_arch = "wasm32")]
    let t = js_sys::Date::now() as u64;
    #[cfg(not(target_arch = "wasm32"))]
    let t = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos() as u64)
        .unwrap_or(1);
    (t ^ (t >> 32)) as u32 | 1
}

pub fn with_rng<R>(f: impl FnOnce(&mut Rng) -> R) -> R {
    RNG.with(|r| f(&mut r.borrow_mut()))
}

#[derive(Clone, Copy)]
pub struct App {
    pub chans: &'static [Signal<ChannelState>],
    pub pattern: Signal<Pattern>,
    pub master: Signal<[f32; NUM_MASTER]>,
    pub transport: Signal<Transport>,
    pub ui: Signal<Ui>,
    pub play: Signal<PlayStatus>,
    pub meters: Signal<Meters>,
    pub clip: Signal<Option<Clip>>,
    pub drag: Signal<Option<KnobDrag>>,
    /// Last (step, value) while drawing in the lane editor.
    pub draw: Signal<Option<(usize, f32)>>,
    /// True between pointer-down in the lane editor and pointer-up anywhere.
    pub drawing: Signal<bool>,
    pub audio_running: Signal<bool>,
}

pub fn use_app() -> App {
    use_context::<App>()
}

impl App {
    /// Must be called from within a component (signals are scope-owned).
    pub fn new() -> Self {
        let chans: Vec<Signal<ChannelState>> = (0..NUM_CH)
            .map(|c| Signal::new(ChannelState::new(c)))
            .collect();
        let mut master = [0.0; NUM_MASTER];
        for (m, s) in master.iter_mut().zip(&MASTER_SPECS) {
            *m = s.default_norm();
        }
        Self {
            chans: Box::leak(chans.into_boxed_slice()),
            pattern: Signal::new(demo_pattern()),
            master: Signal::new(master),
            transport: Signal::new(Transport {
                bpm: 132.0,
                swing: 54.0,
            }),
            ui: Signal::new(Ui {
                ch: KICK,
                var: 0,
                page: 0,
                lane: Lane::Vel,
                loop_mode: false,
                follow: false,
            }),
            play: Signal::new(PlayStatus::default()),
            meters: Signal::new(Meters::default()),
            clip: Signal::new(None),
            drag: Signal::new(None),
            draw: Signal::new(None),
            drawing: Signal::new(false),
            audio_running: Signal::new(true),
        }
    }

    /// Push the complete state to a freshly started engine.
    pub fn sync_all(&self) {
        let mut cmds = Vec::with_capacity(2048);
        let t = *self.transport.peek();
        cmds.push(Command::Bpm(t.bpm));
        cmds.push(Command::Swing(t.swing));
        for (idx, &norm) in self.master.peek().iter().enumerate() {
            cmds.push(Command::MasterParam { idx, norm });
        }
        for (ch, c) in self.chans.iter().enumerate() {
            let c = c.peek();
            for (idx, &norm) in c.synth.iter().enumerate() {
                cmds.push(Command::SynthParam { ch, idx, norm });
            }
            for (idx, &norm) in c.mix.iter().enumerate() {
                cmds.push(Command::MixParam { ch, idx, norm });
            }
            cmds.push(Command::Mute { ch, on: c.mute });
            cmds.push(Command::Solo { ch, on: c.solo });
        }
        let def = Step::default();
        for (ch, vars) in self.pattern.peek().iter().enumerate() {
            for (var, t) in vars.iter().enumerate() {
                cmds.push(Command::TrackLen {
                    ch,
                    var,
                    len: t.len,
                });
                for (step, s) in t.steps.iter().enumerate() {
                    if *s != def {
                        cmds.push(Command::Step {
                            ch,
                            var,
                            step,
                            data: *s,
                        });
                    }
                }
            }
        }
        cmds.push(Command::QueueVar(self.ui.peek().var));
        audio::send_all(&cmds);
    }

    pub fn apply_status(&mut self, s: Status) {
        let p = PlayStatus {
            playing: s.playing,
            var: s.var,
            pending: s.pending,
            steps: s.steps,
        };
        if *self.play.peek() != p {
            self.play.set(p);
        }
        self.meters.set(Meters {
            ch: s.meters,
            peak: s.peak,
            duck_gr: s.duck_gr,
            lim_gr: s.lim_gr,
        });
        let running = audio::running();
        if *self.audio_running.peek() != running {
            self.audio_running.set(running);
        }
    }

    // ------------------------------------------------------- parameters

    pub fn set_synth(&self, ch: usize, idx: usize, norm: f32) {
        let mut c = self.chans[ch];
        c.write().synth[idx] = norm;
        audio::send(Command::SynthParam { ch, idx, norm });
    }

    pub fn set_mix(&self, ch: usize, idx: usize, norm: f32) {
        let mut c = self.chans[ch];
        c.write().mix[idx] = norm;
        audio::send(Command::MixParam { ch, idx, norm });
    }

    pub fn toggle_mute(&self, ch: usize) {
        let mut c = self.chans[ch];
        let on = !c.peek().mute;
        c.write().mute = on;
        audio::send(Command::Mute { ch, on });
    }

    pub fn toggle_solo(&self, ch: usize) {
        let mut c = self.chans[ch];
        let on = !c.peek().solo;
        c.write().solo = on;
        audio::send(Command::Solo { ch, on });
    }

    pub fn set_master(&self, idx: usize, norm: f32) {
        let mut m = self.master;
        m.write()[idx] = norm;
        audio::send(Command::MasterParam { idx, norm });
    }

    pub fn randomize_synth(&self, ch: usize) {
        let specs = CHANNELS[ch].specs;
        let vals: Vec<f32> = with_rng(|r| {
            specs
                .iter()
                .map(|s| {
                    if s.randomizable() {
                        r.range(s.rnd.0, s.rnd.1)
                    } else {
                        s.default_norm()
                    }
                })
                .collect()
        });
        let mut c = self.chans[ch];
        c.write().synth = vals.clone();
        let cmds: Vec<_> = vals
            .iter()
            .enumerate()
            .map(|(idx, &norm)| Command::SynthParam { ch, idx, norm })
            .collect();
        audio::send_all(&cmds);
    }

    pub fn reset_synth(&self, ch: usize) {
        let vals: Vec<f32> = CHANNELS[ch]
            .specs
            .iter()
            .map(|s| s.default_norm())
            .collect();
        let mut c = self.chans[ch];
        c.write().synth = vals.clone();
        let cmds: Vec<_> = vals
            .iter()
            .enumerate()
            .map(|(idx, &norm)| Command::SynthParam { ch, idx, norm })
            .collect();
        audio::send_all(&cmds);
    }

    pub fn audition(&self, ch: usize) {
        audio::resume();
        audio::send(Command::Trigger { ch, vel: 0.9 });
    }

    // -------------------------------------------------------- transport

    pub fn play_pause(&self) {
        audio::resume();
        audio::send(if self.play.peek().playing {
            Command::Pause
        } else {
            Command::Play
        });
    }

    pub fn stop(&self) {
        audio::send(Command::Stop);
    }

    pub fn set_bpm(&self, bpm: f32) {
        let bpm = (bpm * 10.0).round() / 10.0;
        let bpm = bpm.clamp(BPM_MIN, BPM_MAX);
        let mut t = self.transport;
        t.write().bpm = bpm;
        audio::send(Command::Bpm(bpm));
    }

    pub fn set_swing(&self, swing: f32) {
        let mut t = self.transport;
        t.write().swing = swing.clamp(50.0, SWING_MAX);
        audio::send(Command::Swing(swing));
    }

    pub fn select_var(&self, var: usize) {
        let mut ui = self.ui;
        ui.write().var = var;
        let pages = self.pattern.peek()[ui.peek().ch][var].pages;
        if ui.peek().page >= pages {
            ui.write().page = 0;
        }
        audio::send(Command::QueueVar(var));
    }

    pub fn select_channel(&self, ch: usize) {
        let mut ui = self.ui;
        let pages = self.pattern.peek()[ch][ui.peek().var].pages;
        let mut u = ui.write();
        u.ch = ch;
        u.loop_mode = false;
        if u.page >= pages {
            u.page = pages - 1;
        }
    }

    // --------------------------------------------------------- patterns

    /// Edit a track and send exactly the steps that changed.
    pub fn edit_track(&self, ch: usize, var: usize, f: impl FnOnce(&mut Track)) {
        let mut pattern = self.pattern;
        let mut cmds = Vec::new();
        {
            let mut p = pattern.write();
            let t = &mut p[ch][var];
            let before = t.clone();
            f(t);
            t.pages = t.pages.clamp(1, MAX_PAGES);
            t.len = t.len.clamp(1, MAX_STEPS);
            for (step, (a, b)) in before.steps.iter().zip(&t.steps).enumerate() {
                if a != b {
                    cmds.push(Command::Step {
                        ch,
                        var,
                        step,
                        data: *b,
                    });
                }
            }
            if before.len != t.len {
                cmds.push(Command::TrackLen {
                    ch,
                    var,
                    len: t.len,
                });
            }
        }
        audio::send_all(&cmds);
    }

    /// Edit the currently selected track.
    pub fn edit_current(&self, f: impl FnOnce(&mut Track)) {
        let u = self.ui.peek().clone();
        self.edit_track(u.ch, u.var, f);
    }

    pub fn current_track(&self) -> Track {
        let u = self.ui.peek();
        self.pattern.peek()[u.ch][u.var].clone()
    }

    pub fn page_range(&self) -> Range<usize> {
        let p = self.ui.peek().page;
        p * PAGE..(p + 1) * PAGE
    }

    pub fn set_page(&self, page: usize) {
        let mut ui = self.ui;
        ui.write().page = page.min(MAX_PAGES - 1);
    }

    pub fn change_pages(&self, delta: i32) {
        let t = self.current_track();
        let pages = (t.pages as i32 + delta).clamp(1, MAX_PAGES as i32) as usize;
        self.edit_current(|t| t.set_pages(pages));
        let mut ui = self.ui;
        if ui.peek().page >= pages {
            ui.write().page = pages - 1;
        }
    }

    pub fn duplicate(&self) {
        self.edit_current(|t| t.duplicate());
    }

    pub fn copy_page(&self) {
        let t = self.current_track();
        let mut clip = self.clip;
        clip.set(Some(Clip::Page(t.steps[self.page_range()].to_vec())));
    }

    pub fn copy_track(&self) {
        let mut clip = self.clip;
        clip.set(Some(Clip::Track(self.current_track())));
    }

    pub fn paste(&self) {
        let Some(clip) = self.clip.peek().clone() else {
            return;
        };
        let range = self.page_range();
        match clip {
            Clip::Page(steps) => self.edit_current(|t| {
                t.steps[range].copy_from_slice(&steps);
            }),
            Clip::Track(src) => self.edit_current(|t| *t = src),
        }
    }

    pub fn clear_page(&self) {
        let range = self.page_range();
        self.edit_current(|t| t.steps[range].iter_mut().for_each(|s| *s = Step::default()));
    }

    pub fn shift(&self, dir: i32) {
        self.edit_current(|t| {
            let n = t.len;
            if dir > 0 {
                t.steps[..n].rotate_right(1);
            } else {
                t.steps[..n].rotate_left(1);
            }
        });
    }

    pub fn toggle_step(&self, i: usize) {
        if self.ui.peek().loop_mode {
            let mut ui = self.ui;
            self.edit_current(|t| t.len = (i + 1).min(t.pages * PAGE));
            ui.write().loop_mode = false;
            return;
        }
        self.edit_current(|t| t.steps[i].on = !t.steps[i].on);
    }

    /// `v` is the normalized lane height (0 = bottom, 1 = top). Pitch is free
    /// (unquantized) unless `snap` is set, which rounds to semitones.
    pub fn set_lane_value(&self, i: usize, v: f32, snap: bool) {
        let lane = self.ui.peek().lane;
        self.edit_current(|t| {
            let s = &mut t.steps[i];
            match lane {
                Lane::Vel => s.vel = v.clamp(0.02, 1.0),
                Lane::Prob => s.prob = v.clamp(0.0, 1.0),
                Lane::Pitch => {
                    let p = ((v - 0.5) * 2.0 * PITCH_RANGE).clamp(-PITCH_RANGE, PITCH_RANGE);
                    s.pitch = if snap {
                        p.round()
                    } else {
                        (p * 100.0).round() / 100.0
                    };
                }
            }
        });
    }

    /// Normalized lane height of step `i` in the shown lane (inverse of `set_lane_value`).
    pub fn lane_norm(&self, i: usize) -> f32 {
        let u = self.ui.peek();
        let s = self.pattern.peek()[u.ch][u.var].steps[i];
        match u.lane {
            Lane::Vel => s.vel,
            Lane::Prob => s.prob,
            Lane::Pitch => s.pitch / (2.0 * PITCH_RANGE) + 0.5,
        }
    }

    /// Nudge step `i` in the shown lane: one fine unit (0.1 st, 1 velocity
    /// value, 1 %) times `units`.
    pub fn nudge_lane(&self, i: usize, units: f32) {
        let unit = match self.ui.peek().lane {
            Lane::Vel => 1.0 / 127.0,
            Lane::Prob => 0.01,
            Lane::Pitch => 0.1 / (2.0 * PITCH_RANGE),
        };
        self.set_lane_value(i, self.lane_norm(i) + unit * units, false);
    }

    /// Draw from the last point to `(i, v)`, filling skipped steps.
    pub fn draw_to(&self, i: usize, v: f32, snap: bool) {
        let mut draw = self.draw;
        let last = *draw.peek();
        match last {
            Some((j, w)) if j != i => {
                let (a, b) = if j < i { (j, i) } else { (i, j) };
                for k in a..=b {
                    let t = (k as f32 - j as f32) / (i as f32 - j as f32);
                    self.set_lane_value(k, w + (v - w) * t, snap);
                }
            }
            _ => self.set_lane_value(i, v, snap),
        }
        draw.set(Some((i, v)));
    }

    pub fn randomize_page_steps(&self) {
        let ch = self.ui.peek().ch;
        let range = self.page_range();
        self.edit_current(|t| with_rng(|r| randomize_steps(t, ch, 0.5, range, r)));
    }

    pub fn randomize_lane(&self) {
        let (ch, lane) = {
            let u = self.ui.peek();
            (u.ch, u.lane)
        };
        let range = self.page_range();
        self.edit_current(|t| {
            with_rng(|r| match lane {
                Lane::Vel => randomize_vel(t, range, r),
                Lane::Pitch => randomize_pitch(t, ch, range, r),
                Lane::Prob => randomize_prob(t, range, r),
            })
        });
    }

    pub fn mutate_page(&self) {
        let ch = self.ui.peek().ch;
        let range = self.page_range();
        self.edit_current(|t| with_rng(|r| mutate(t, ch, range, r)));
    }

    /// New rhythm for every track of the edited variation.
    pub fn randomize_kit(&self) {
        let var = self.ui.peek().var;
        let density = with_rng(|r| r.range(0.35, 0.65));
        for ch in 0..NUM_CH {
            self.edit_track(ch, var, |t| {
                let n = t.pages * PAGE;
                with_rng(|r| {
                    let d = if matches!(ch, 6..=11) {
                        density * 0.6
                    } else {
                        density
                    };
                    randomize_steps(t, ch, d, 0..n, r);
                    if ch == STAB {
                        randomize_pitch(t, ch, 0..n, r);
                    }
                });
            });
        }
    }
}
