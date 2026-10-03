use crate::state::use_app;
use crate::ui::knob::Knob;
use dioxus::prelude::*;
use rustbelt_engine::params::*;

const VAR_NAMES: [&str; 3] = ["A", "B", "C"];

#[component]
pub fn Header() -> Element {
    let app = use_app();
    let play = app.play.read();
    let t = *app.transport.read();
    let ui_var = app.ui.read().var;
    let bpm_norm = (t.bpm - BPM_MIN) / (BPM_MAX - BPM_MIN);
    let swing_norm = (t.swing - 50.0) / (SWING_MAX - 50.0);
    let beat = play.steps[0].max(0) / 4 % 4;

    rsx! {
        header { class: "header",
            div { class: "logo",
                span { class: "logo-main", "RUSTBELT" }
                span { class: "logo-model", "RB-128" }
                span { class: "logo-sub", "techno session groovebox" }
            }
            div { class: "transport",
                button {
                    class: if play.playing { "tbtn play on" } else { "tbtn play" },
                    title: "Play / pause (space)",
                    onclick: move |_| app.play_pause(),
                    if play.playing { "❚❚" } else { "▶" }
                }
                button { class: "tbtn stop", title: "Stop and rewind", onclick: move |_| app.stop(), "■" }
                div { class: "beat-leds",
                    for i in 0..4 {
                        span { class: if play.playing && beat == i { "beat-led on" } else { "beat-led" } }
                    }
                }
                div { class: "bpm",
                    button { class: "mini", onclick: move |_| app.set_bpm(t.bpm - 1.0), "−" }
                    div { class: "bpm-display",
                        span { class: "bpm-value", "{t.bpm:.1}" }
                        span { class: "bpm-unit", "BPM" }
                    }
                    button { class: "mini", onclick: move |_| app.set_bpm(t.bpm + 1.0), "+" }
                }
                Knob {
                    label: "Tempo",
                    value: bpm_norm,
                    default: (132.0 - BPM_MIN) / (BPM_MAX - BPM_MIN),
                    display: format!("{:.0}", t.bpm),
                    onchange: move |v: f32| app.set_bpm(BPM_MIN + v * (BPM_MAX - BPM_MIN)),
                    size: 34.0,
                }
                Knob {
                    label: "Swing",
                    value: swing_norm,
                    default: 0.0,
                    display: format!("{:.0}%", t.swing),
                    onchange: move |v: f32| app.set_swing(50.0 + v * (SWING_MAX - 50.0)),
                    size: 34.0,
                }
            }
            div { class: "variations",
                span { class: "section-label", "VARIATION" }
                for v in 0..3 {
                    button {
                        class: {
                            let mut c = String::from("var-btn");
                            if play.var == v { c.push_str(" playing"); }
                            if play.pending == Some(v) { c.push_str(" pending"); }
                            if ui_var == v { c.push_str(" editing"); }
                            c
                        },
                        title: "Cue variation {VAR_NAMES[v]} (switches when the longest loop wraps)",
                        onclick: move |_| app.select_var(v),
                        "{VAR_NAMES[v]}"
                    }
                }
            }
            MasterMeter {}
        }
    }
}

#[component]
fn MasterMeter() -> Element {
    let app = use_app();
    let m = *app.meters.read();
    let w = |p: f32| {
        let db = 20.0 * p.max(1e-5).log10();
        ((db + 48.0) / 48.0 * 100.0).clamp(0.0, 100.0)
    };
    let gr = |g: f32| (g / 18.0 * 100.0).clamp(0.0, 100.0);
    rsx! {
        div { class: "master-meter",
            div { class: "mm-row",
                span { "L" }
                div { class: "mm-bar", div { class: "mm-fill", style: "width:{w(m.peak[0])}%" } }
            }
            div { class: "mm-row",
                span { "R" }
                div { class: "mm-bar", div { class: "mm-fill", style: "width:{w(m.peak[1])}%" } }
            }
            div { class: "mm-row gr",
                span { "DK" }
                div { class: "mm-bar", div { class: "mm-gr", style: "width:{gr(m.duck_gr)}%" } }
            }
            div { class: "mm-row gr",
                span { "LM" }
                div { class: "mm-bar", div { class: "mm-gr", style: "width:{gr(m.lim_gr)}%" } }
            }
        }
    }
}
