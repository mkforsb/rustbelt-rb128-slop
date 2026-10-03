use crate::state::use_app;
use crate::ui::knob::SpecKnob;
use crate::ui::CH_COLORS;
use dioxus::prelude::*;
use rustbelt_engine::params::*;
use std::ops::Range;

fn groups(kind: Kind) -> &'static [(&'static str, Range<usize>)] {
    match kind {
        Kind::Kick => &[
            ("BODY", 0..4),
            ("HIT", 4..6),
            ("SUB", 6..8),
            ("DIST", 8..9),
            ("RUMBLE · ducked by kick", 9..15),
        ],
        Kind::Hat => &[("METAL", 0..5)],
        Kind::Ride => &[("METAL", 0..6)],
        Kind::Snare => &[("BODY", 0..3), ("SNAPPY", 3..6)],
        Kind::Clap => &[("CLAP", 0..5)],
        Kind::Perc => &[("VOICE", 0..5)],
        Kind::Stab => &[
            ("VOICES · free tuning", 0..4),
            ("FM", 4..8),
            ("AMP", 8..10),
            ("FILTER", 10..12),
        ],
    }
}

#[component]
pub fn InstrumentPanel() -> Element {
    let app = use_app();
    let ch = app.ui.read().ch;
    let def = &CHANNELS[ch];
    let c = app.chans[ch].read().clone();
    let color = CH_COLORS[ch];
    rsx! {
        section { class: "panel instrument", style: "--ch:{color}",
            div { class: "panel-title",
                span { class: "inst-dot" }
                "{def.name.to_uppercase()}"
                div { class: "panel-actions",
                    button {
                        class: "act",
                        title: "Trigger this voice",
                        onpointerdown: move |_| app.audition(ch),
                        "▶ HIT"
                    }
                    button { class: "act", title: "Randomize sound", onclick: move |_| app.randomize_synth(ch), "⚄ RND" }
                    button { class: "act", title: "Back to the factory sound", onclick: move |_| app.reset_synth(ch), "INIT" }
                }
            }
            div { class: "groups",
                for (gname , range) in groups(def.kind).iter().cloned() {
                    div { class: "group",
                        div { class: "group-label", "{gname}" }
                        div { class: "group-knobs",
                            for idx in range {
                                SpecKnob {
                                    key: "{ch}-{idx}",
                                    spec: &def.specs[idx],
                                    value: c.synth[idx],
                                    onchange: move |v| app.set_synth(ch, idx, v),
                                    size: 40.0,
                                    accent: color.to_string(),
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}
