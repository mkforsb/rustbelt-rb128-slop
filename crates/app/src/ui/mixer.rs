use crate::state::use_app;
use crate::ui::knob::SpecKnob;
use crate::ui::{CH_COLORS, MUTE_KEYS};
use dioxus::prelude::*;
use rustbelt_engine::params::*;

#[component]
pub fn Mixer() -> Element {
    rsx! {
        section { class: "panel mixer",
            div { class: "panel-title", "MIXER" }
            div { class: "strips",
                for ch in 0..NUM_CH {
                    Strip { key: "{ch}", ch }
                }
            }
        }
    }
}

#[component]
fn Strip(ch: usize) -> Element {
    let app = use_app();
    let c = app.chans[ch].read().clone();
    let selected = app.ui.read().ch == ch;
    let color = CH_COLORS[ch];
    let knob = |idx: usize| {
        rsx! {
            SpecKnob {
                spec: &MIX_SPECS[idx],
                value: c.mix[idx],
                onchange: move |v| app.set_mix(ch, idx, v),
                size: 30.0,
                accent: color.to_string(),
            }
        }
    };
    rsx! {
        div { class: if selected { "strip selected" } else { "strip" }, style: "--ch:{color}",
            button {
                class: "strip-name",
                title: "{CHANNELS[ch].name}: select for editing",
                onclick: move |_| app.select_channel(ch),
                "{CHANNELS[ch].short}"
                span { class: "key-hint", title: "Mute key", "{MUTE_KEYS[ch]}" }
            }
            div { class: "strip-btns",
                button {
                    class: if c.mute { "sbtn mute on" } else { "sbtn mute" },
                    title: "Mute: stops new notes, tails ring out (key {MUTE_KEYS[ch]})",
                    onclick: move |_| app.toggle_mute(ch),
                    "M"
                }
                button {
                    class: if c.solo { "sbtn solo on" } else { "sbtn solo" },
                    title: "Solo: only soloed tracks trigger notes",
                    onclick: move |_| app.toggle_solo(ch),
                    "S"
                }
                button {
                    class: "sbtn dice",
                    title: "Randomize {CHANNELS[ch].name} sound",
                    onclick: move |_| app.randomize_synth(ch),
                    "⚄"
                }
            }
            div { class: "strip-knobs",
                {knob(MIX_EQ_HI)}
                {knob(MIX_DLY)}
                {knob(MIX_EQ_MID)}
                {knob(MIX_REV)}
                {knob(MIX_EQ_LO)}
                {knob(MIX_PAN)}
            }
            div { class: "strip-level",
                ChannelMeter { ch }
                SpecKnob {
                    spec: &MIX_SPECS[MIX_LEVEL],
                    value: c.mix[MIX_LEVEL],
                    onchange: move |v| app.set_mix(ch, MIX_LEVEL, v),
                    size: 38.0,
                    accent: color.to_string(),
                }
            }
        }
    }
}

#[component]
pub fn ChannelMeter(ch: usize) -> Element {
    let app = use_app();
    let p = app.meters.read().ch[ch];
    let db = 20.0 * p.max(1e-5).log10();
    let h = ((db + 42.0) / 42.0 * 100.0).clamp(0.0, 100.0);
    rsx! {
        div { class: "ch-meter", div { class: "ch-meter-fill", style: "height:{h}%" } }
    }
}

/// Small LED that lights with the channel's output level.
#[component]
pub fn ChannelLed(ch: usize) -> Element {
    let app = use_app();
    let p = app.meters.read().ch[ch];
    let a = (p * 2.5).clamp(0.0, 1.0);
    rsx! {
        span { class: "led", style: "opacity:{0.15 + 0.85 * a}" }
    }
}
