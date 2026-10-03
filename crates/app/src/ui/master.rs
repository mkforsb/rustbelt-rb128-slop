use crate::state::use_app;
use crate::ui::knob::SpecKnob;
use dioxus::prelude::*;
use rustbelt_engine::params::*;

const GROUPS: &[(&str, &[usize])] = &[
    (
        "DELAY · synced",
        &[DLY_TIME, DLY_FB, DLY_HICUT, DLY_LOCUT, DLY_WIDTH, DLY_LEVEL],
    ),
    (
        "REVERB",
        &[REV_DECAY, REV_DAMP, REV_PRE, REV_LOCUT, REV_LEVEL],
    ),
    (
        "DUCKER · kick key",
        &[DUCK_THRESH, DUCK_RATIO, DUCK_ATTACK, DUCK_RELEASE],
    ),
    ("LIMITER", &[LIM_MAKEUP, LIM_CEIL, LIM_RELEASE, MASTER_VOL]),
];

#[component]
pub fn MasterPanel() -> Element {
    let app = use_app();
    let m = *app.master.read();
    let knob = move |idx: usize, size: f32| {
        rsx! {
            SpecKnob {
                key: "{idx}",
                spec: &MASTER_SPECS[idx],
                value: m[idx],
                onchange: move |v| app.set_master(idx, v),
                size,
            }
        }
    };
    rsx! {
        section { class: "panel master",
            div { class: "panel-title", "MASTER" }
            div { class: "groups",
                for (name , idxs) in GROUPS.iter() {
                    div { class: "group",
                        div { class: "group-label", "{name}" }
                        div { class: "group-knobs",
                            for &idx in idxs.iter() {
                                {knob(idx, 36.0)}
                            }
                            if name.starts_with("DUCKER") {
                                GrMeter { duck: true }
                            }
                            if name.starts_with("LIMITER") {
                                GrMeter { duck: false }
                            }
                        }
                    }
                }
                div { class: "group dj",
                    div { class: "group-label", "DJ FILTER · LP ◂ ▸ HP" }
                    div { class: "group-knobs",
                        {knob(FILTER, 58.0)}
                        {knob(FILTER_RES, 36.0)}
                    }
                }
            }
        }
    }
}

#[component]
fn GrMeter(duck: bool) -> Element {
    let app = use_app();
    let m = app.meters.read();
    let gr = if duck { m.duck_gr } else { m.lim_gr };
    let h = (gr / 24.0 * 100.0).clamp(0.0, 100.0);
    rsx! {
        div { class: "gr-meter", title: "Gain reduction",
            div { class: "gr-bar", div { class: "gr-fill", style: "height:{h}%" } }
            div { class: "gr-text", if gr < 0.5 { "0" } else { "-{gr:.0}" } }
        }
    }
}
