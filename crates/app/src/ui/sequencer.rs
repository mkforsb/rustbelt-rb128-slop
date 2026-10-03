use crate::state::{use_app, Clip, Lane};
use crate::ui::mixer::ChannelLed;
use crate::ui::CH_COLORS;
use dioxus::prelude::*;
use rustbelt_engine::params::*;
use rustbelt_engine::seq::*;
use std::rc::Rc;

#[component]
pub fn Sequencer() -> Element {
    rsx! {
        section { class: "panel sequencer",
            TrackSelect {}
            SeqTools {}
            StepRow {}
            LaneEditor {}
        }
    }
}

#[component]
fn TrackSelect() -> Element {
    let app = use_app();
    let sel = app.ui.read().ch;
    let var = app.ui.read().var;
    let pattern = app.pattern.read();
    rsx! {
        div { class: "track-select",
            span { class: "section-label", "TRACK" }
            for ch in 0..NUM_CH {
                {
                    let t = &pattern[ch][var];
                    let used = t.steps[..t.pages * PAGE].iter().any(|s| s.on);
                    let poly = !t.len.is_multiple_of(PAGE);
                    rsx! {
                        button {
                            key: "{ch}",
                            class: if sel == ch { "track-btn selected" } else { "track-btn" },
                            style: "--ch:{CH_COLORS[ch]}",
                            title: "{CHANNELS[ch].name}",
                            onclick: move |_| app.select_channel(ch),
                            ChannelLed { ch }
                            span { class: "track-name", "{CHANNELS[ch].short}" }
                            span { class: "track-info",
                                if used { "{t.len}" } else { "·" }
                                if poly { "∿" }
                            }
                        }
                    }
                }
            }
        }
    }
}

fn shown_page(app: &crate::state::App) -> usize {
    let ui = app.ui.read();
    let play = app.play.read();
    if ui.follow && play.playing && play.var == ui.var && play.steps[ui.ch] >= 0 {
        play.steps[ui.ch] as usize / PAGE
    } else {
        ui.page
    }
}

#[component]
fn SeqTools() -> Element {
    let app = use_app();
    let ui = app.ui.read().clone();
    let track = app.pattern.read()[ui.ch][ui.var].clone();
    let play = *app.play.read();
    let page = shown_page(&app);
    let playing_page = (play.playing && play.var == ui.var && play.steps[ui.ch] >= 0)
        .then(|| play.steps[ui.ch] as usize / PAGE);
    let clip = app.clip.read().clone();
    let clip_label = match clip {
        Some(Clip::Page(_)) => "PASTE PG",
        Some(Clip::Track(_)) => "PASTE TRK",
        None => "PASTE",
    };

    rsx! {
        div { class: "seq-tools",
            div { class: "tool-group",
                span { class: "section-label", "PAGE" }
                for p in 0..MAX_PAGES {
                    button {
                        class: {
                            let mut c = String::from("page-btn");
                            if p >= track.pages { c.push_str(" unused"); }
                            if p == page { c.push_str(" current"); }
                            if playing_page == Some(p) { c.push_str(" playing"); }
                            c
                        },
                        disabled: p >= track.pages,
                        onclick: move |_| app.set_page(p),
                        "{p + 1}"
                    }
                }
                button {
                    class: if ui.follow { "tool on" } else { "tool" },
                    title: "Pages follow the playhead",
                    onclick: move |_| {
                        let mut u = app.ui;
                        let f = !u.peek().follow;
                        u.write().follow = f;
                    },
                    "FOLLOW"
                }
            }
            div { class: "tool-group",
                span { class: "section-label", "LENGTH" }
                button { class: "tool", title: "Fewer pages", onclick: move |_| app.change_pages(-1), "−" }
                span { class: "len-display", "{track.pages * PAGE}" }
                button { class: "tool", title: "More pages", onclick: move |_| app.change_pages(1), "+" }
                button {
                    class: if ui.loop_mode { "tool on blink" } else { "tool" },
                    title: "Set a loop point: press LOOP, then the last step of the loop (polyrhythms)",
                    onclick: move |_| {
                        let mut u = app.ui;
                        let m = !u.peek().loop_mode;
                        u.write().loop_mode = m;
                    },
                    "LOOP"
                }
                span { class: if !track.len.is_multiple_of(PAGE) { "len-display poly" } else { "len-display" }, "⟲ {track.len}" }
            }
            div { class: "tool-group",
                span { class: "section-label", "EDIT" }
                button { class: "tool", title: "Copy this page", onclick: move |_| app.copy_page(), "COPY PG" }
                button { class: "tool", title: "Copy the whole track (all pages)", onclick: move |_| app.copy_track(), "COPY TRK" }
                button {
                    class: "tool",
                    disabled: clip.is_none(),
                    title: "Paste into this page / track (works across tracks and variations)",
                    onclick: move |_| app.paste(),
                    "{clip_label}"
                }
                button { class: "tool", title: "Duplicate: 16→32→64→128 steps", onclick: move |_| app.duplicate(), "×2" }
                button { class: "tool", title: "Clear this page", onclick: move |_| app.clear_page(), "CLR" }
                button { class: "tool", title: "Shift track left", onclick: move |_| app.shift(-1), "◂" }
                button { class: "tool", title: "Shift track right", onclick: move |_| app.shift(1), "▸" }
            }
            div { class: "tool-group",
                span { class: "section-label", "RANDOM" }
                button { class: "tool", title: "New rhythm on this page", onclick: move |_| app.randomize_page_steps(), "STEPS" }
                button { class: "tool", title: "Randomize the lane shown below on this page", onclick: move |_| app.randomize_lane(), "LANE" }
                button { class: "tool", title: "Flip a few steps", onclick: move |_| app.mutate_page(), "MUTATE" }
                button { class: "tool warn", title: "New rhythm for every track of this variation", onclick: move |_| app.randomize_kit(), "ALL TRACKS" }
            }
        }
    }
}

#[component]
fn StepRow() -> Element {
    let app = use_app();
    let ui = app.ui.read().clone();
    let pattern = app.pattern.read();
    let track = &pattern[ui.ch][ui.var];
    let play = *app.play.read();
    let page = shown_page(&app);
    let head = if play.playing && play.var == ui.var {
        play.steps[ui.ch]
    } else {
        -1
    };

    rsx! {
        div { class: if ui.loop_mode { "steps loop-mode" } else { "steps" }, style: "--ch:{CH_COLORS[ui.ch]}",
            for k in 0..PAGE {
                {
                    let i = page * PAGE + k;
                    let s = track.steps[i];
                    let mut class = format!("step g{}", k / 4);
                    if s.on { class.push_str(" on"); }
                    if head == i as i32 { class.push_str(" head"); }
                    if i >= track.len { class.push_str(" outside"); }
                    if i + 1 == track.len { class.push_str(" loop-end"); }
                    let vel_h = s.vel * 100.0;
                    rsx! {
                        button {
                            key: "{k}",
                            class,
                            onclick: move |_| app.toggle_step(i),
                            span { class: "step-num", "{i + 1}" }
                            span { class: "step-led" }
                            if s.on {
                                span { class: "step-vel", style: "height:{vel_h}%" }
                            }
                            span { class: "step-flags",
                                if s.pitch.abs() >= 0.005 { "{s.pitch:+.1}" }
                                if s.prob < 1.0 { " {(s.prob * 100.0).round()}%" }
                            }
                        }
                    }
                }
            }
        }
    }
}

/// What a pointer gesture in the lane editor does.
#[derive(Clone, Copy, PartialEq)]
enum Gesture {
    /// Draw absolute values, interpolating across skipped steps.
    Draw { snap: bool },
    /// Shift-drag: adjust one step relative to its value, at 1/10 speed.
    Fine { i: usize, y0: f64, v0: f32 },
}

/// Fine-drag sensitivity relative to drawing.
const FINE: f64 = 0.1;

#[component]
fn LaneEditor() -> Element {
    let app = use_app();
    let ui = app.ui.read().clone();
    let pattern = app.pattern.read();
    let track = &pattern[ui.ch][ui.var];
    let page = shown_page(&app);
    let lanes = [
        (Lane::Vel, "VELOCITY"),
        (Lane::Pitch, "PITCH"),
        (Lane::Prob, "PROBABILITY"),
    ];

    // Pointer positions are mapped with client coordinates against the lane's
    // live bounding rect, so the mapping is immune to zoom, HiDPI scaling and
    // engine differences in `offsetX/Y`. The rect is re-measured on every
    // pointer-down; points arriving before it does are queued.
    let mut lane_el = use_signal(|| None::<Rc<MountedData>>);
    let mut rect = use_signal(|| None::<[f64; 4]>);
    let mut pending = use_signal(Vec::<(f64, f64)>::new);
    // (fine, snap) requested at pointer-down, and the gesture once started
    let mut request = use_signal(|| (false, false));
    let mut gesture = use_signal(|| None::<Gesture>);

    let apply = move |x: f64, y: f64| {
        let Some([rx, ry, rw, rh]) = *rect.peek() else {
            return;
        };
        if rw <= 0.0 || rh <= 0.0 {
            return;
        }
        let col = ((x - rx) / rw * PAGE as f64)
            .floor()
            .clamp(0.0, (PAGE - 1) as f64) as usize;
        let v = (1.0 - (y - ry) / rh).clamp(0.0, 1.0) as f32;
        let mut g = gesture; // local copy keeps `apply` a plain `Fn`
        let current = *g.peek();
        match current {
            None => {
                let (fine, snap) = *request.peek();
                if fine {
                    let i = page * PAGE + col;
                    g.set(Some(Gesture::Fine {
                        i,
                        y0: y,
                        v0: app.lane_norm(i),
                    }));
                } else {
                    g.set(Some(Gesture::Draw { snap }));
                    app.draw_to(page * PAGE + col, v, snap);
                }
            }
            Some(Gesture::Draw { snap }) => app.draw_to(page * PAGE + col, v, snap),
            Some(Gesture::Fine { i, y0, v0 }) => {
                let dv = ((y0 - y) / rh * FINE) as f32;
                app.set_lane_value(i, v0 + dv, false);
            }
        }
    };

    let hint = match ui.lane {
        Lane::Pitch => "free tuning ±24 st in 0.1 steps · shift-drag = fine · ctrl = snap to semitones · wheel = ±1, shift+wheel = ±0.1",
        _ => "draw curves · shift-drag = fine · wheel nudges (shift = finer) · double-click resets",
    };

    rsx! {
        div { class: "lane-wrap",
            div { class: "lane-tabs",
                for (lane , name) in lanes {
                    button {
                        class: if ui.lane == lane { "lane-tab on" } else { "lane-tab" },
                        onclick: move |_| {
                            let mut u = app.ui;
                            u.write().lane = lane;
                        },
                        "{name}"
                    }
                }
                span { class: "lane-hint", "{hint}" }
            }
            div {
                class: "lane",
                style: "--ch:{CH_COLORS[ui.ch]}",
                onmounted: move |e: MountedEvent| lane_el.set(Some(e.data())),
                onpointerdown: move |e: PointerEvent| {
                    e.prevent_default();
                    let (mut draw, mut drawing) = (app.draw, app.drawing);
                    draw.set(None);
                    drawing.set(true);
                    rect.set(None);
                    gesture.set(None);
                    let m = e.modifiers();
                    request.set((m.shift(), m.ctrl() || m.meta()));
                    let c = e.client_coordinates();
                    pending.write().push((c.x, c.y));
                    if let Some(el) = lane_el.peek().clone() {
                        spawn(async move {
                            if let Ok(r) = el.get_client_rect().await {
                                rect.set(Some([r.origin.x, r.origin.y, r.size.width, r.size.height]));
                            }
                            let pts: Vec<_> = pending.write().drain(..).collect();
                            for (x, y) in pts {
                                apply(x, y);
                            }
                        });
                    }
                },
                onpointermove: move |e: PointerEvent| {
                    if !*app.drawing.peek() {
                        return;
                    }
                    let c = e.client_coordinates();
                    if rect.peek().is_some() {
                        apply(c.x, c.y);
                    } else {
                        pending.write().push((c.x, c.y));
                    }
                },
                if ui.lane == Lane::Pitch {
                    div { class: "lane-zero" }
                }
                for k in 0..PAGE {
                    {
                        let i = page * PAGE + k;
                        let s = track.steps[i];
                        let mut class = String::from("lane-col");
                        if !s.on {
                            class.push_str(" off");
                        }
                        if i >= track.len {
                            class.push_str(" outside");
                        }
                        // (top %, height %) — always the same CSS properties: Dioxus
                        // patches style strings property by property, so a property
                        // that disappears from the string would otherwise linger.
                        let (top, height, label) = match ui.lane {
                            Lane::Vel => {
                                let v = s.vel.clamp(0.0, 1.0) * 100.0;
                                (100.0 - v, v, format!("{:.0}", s.vel.clamp(0.0, 1.0) * 127.0))
                            }
                            Lane::Prob => {
                                let v = s.prob.clamp(0.0, 1.0) * 100.0;
                                (100.0 - v, v, format!("{v:.0}%"))
                            }
                            Lane::Pitch => {
                                let p = s.pitch.clamp(-PITCH_RANGE, PITCH_RANGE);
                                let h = p.abs() / PITCH_RANGE * 50.0;
                                let top = if p >= 0.0 { 50.0 - h } else { 50.0 };
                                let label = if p.abs() < 0.05 { "0".to_string() } else { format!("{p:+.1}") };
                                (top, h, label)
                            }
                        };
                        let bar_style = format!("top:{top:.3}%;height:{height:.3}%;bottom:auto");
                        rsx! {
                            div {
                                key: "{k}",
                                class,
                                onwheel: move |e: WheelEvent| {
                                    e.prevent_default();
                                    let d = e.delta().strip_units();
                                    let dy = if d.y != 0.0 { d.y } else { d.x };
                                    if dy != 0.0 {
                                        let units = if e.modifiers().shift() { 1.0 } else { 10.0 };
                                        app.nudge_lane(i, -(dy.signum() as f32) * units);
                                    }
                                },
                                ondoubleclick: move |_| {
                                    let def = Step::default();
                                    let lane = app.ui.peek().lane;
                                    app.edit_current(|t| {
                                        let st = &mut t.steps[i];
                                        match lane {
                                            Lane::Vel => st.vel = def.vel,
                                            Lane::Pitch => st.pitch = def.pitch,
                                            Lane::Prob => st.prob = def.prob,
                                        }
                                    });
                                },
                                div { class: "lane-bar", style: bar_style }
                                span { class: "lane-label", "{label}" }
                            }
                        }
                    }
                }
            }
        }
    }
}
