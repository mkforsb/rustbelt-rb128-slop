use crate::state::{use_app, App, KnobDrag};
use dioxus::prelude::*;
use rustbelt_engine::params::{Map, ParamSpec, Unit};

fn quantize(v: f32, step: f32) -> f32 {
    if step > 0.0 {
        ((v / step).round() * step).clamp(0.0, 1.0)
    } else {
        v
    }
}

const SWEEP: f32 = 270.0;

fn polar(cx: f32, cy: f32, r: f32, deg: f32) -> (f32, f32) {
    let a = deg.to_radians();
    (cx + r * a.sin(), cy - r * a.cos())
}

fn arc(cx: f32, cy: f32, r: f32, from: f32, to: f32) -> String {
    let (a, b) = if from <= to { (from, to) } else { (to, from) };
    if b - a < 0.01 {
        return String::new();
    }
    let (x0, y0) = polar(cx, cy, r, a);
    let (x1, y1) = polar(cx, cy, r, b);
    let large = if b - a > 180.0 { 1 } else { 0 };
    format!("M{x0:.2},{y0:.2} A{r},{r} 0 {large} 1 {x1:.2},{y1:.2}")
}

/// Rotary control: drag vertically (shift = fine), wheel, double-click resets.
#[component]
pub fn Knob(
    label: String,
    value: f32,
    default: f32,
    display: String,
    onchange: EventHandler<f32>,
    #[props(default)] bipolar: bool,
    #[props(default = 36.0)] size: f32,
    #[props(default)] accent: Option<String>,
    /// Quantum in normalized units (0 = continuous). The wheel moves 10
    /// quanta per notch, or one with shift.
    #[props(default)]
    step: f32,
) -> Element {
    let app = use_app();
    let mut drag = app.drag;
    let active = (*drag.read()).is_some_and(|d| d.onchange == onchange);
    let s = size;
    let c = s / 2.0;
    let r = s / 2.0 - 3.5;
    let start = -SWEEP / 2.0;
    let ang = start + SWEEP * value.clamp(0.0, 1.0);
    let from = if bipolar { 0.0 } else { start };
    let (px, py) = polar(c, c, r * 0.62, ang);
    let (qx, qy) = polar(c, c, r * 0.18, ang);
    let style = accent.map(|a| format!("--accent:{a}")).unwrap_or_default();

    rsx! {
        div {
            class: if active { "knob active" } else { "knob" },
            style,
            title: "{label}: {display} (drag or wheel, shift = fine, double-click = reset)",
            div { class: "knob-label", "{label}" }
            svg {
                width: "{s}",
                height: "{s}",
                view_box: "0 0 {s} {s}",
                onpointerdown: move |e: PointerEvent| {
                    e.prevent_default();
                    drag.set(Some(KnobDrag { start_y: e.client_coordinates().y, start: value, onchange, step }));
                },
                ondoubleclick: move |_| onchange.call(default),
                onwheel: move |e: WheelEvent| {
                    e.prevent_default();
                    // shift+wheel is reported as horizontal scrolling by many platforms
                    let d = e.delta().strip_units();
                    let dy = if d.y != 0.0 { d.y } else { d.x };
                    if dy != 0.0 {
                        let fine = e.modifiers().shift();
                        let inc = match (step > 0.0, fine) {
                            (true, true) => step,
                            (true, false) => step * 10.0,
                            (false, true) => 0.005,
                            (false, false) => 0.025,
                        };
                        onchange.call(quantize((value - inc * dy.signum() as f32).clamp(0.0, 1.0), step));
                    }
                },
                circle { class: "knob-body", cx: "{c}", cy: "{c}", r: "{r - 2.5}" }
                path { class: "knob-track", d: arc(c, c, r, start, start + SWEEP) }
                path { class: "knob-value", d: arc(c, c, r, from, ang) }
                line { class: "knob-pointer", x1: "{qx}", y1: "{qy}", x2: "{px}", y2: "{py}" }
            }
            div { class: "knob-value-text", "{display}" }
        }
    }
}

/// Knob bound to a `ParamSpec` (handles formatting, default and polarity).
#[component]
pub fn SpecKnob(
    spec: &'static ParamSpec,
    value: f32,
    onchange: EventHandler<f32>,
    #[props(default = 36.0)] size: f32,
    #[props(default)] accent: Option<String>,
) -> Element {
    let bipolar = matches!(spec.map, Map::Lin(a, b) if a < 0.0 && b > 0.0 && (a + b).abs() < 1e-3);
    // Pitches are free but settle on 0.1 semitone so values like +12.4 are exact.
    let step = match (spec.unit, spec.map) {
        (Unit::Semi, Map::Lin(a, b)) => 0.1 / (b - a),
        _ => 0.0,
    };
    rsx! {
        Knob {
            label: spec.name.to_string(),
            value,
            default: spec.default_norm(),
            display: spec.format(value),
            onchange,
            bipolar,
            size,
            accent,
            step,
        }
    }
}

/// Root-level pointer handlers that drive whichever knob is being dragged.
pub fn drag_move(app: &App, e: &PointerEvent) {
    if let Some(d) = *app.drag.peek() {
        let dy = (d.start_y - e.client_coordinates().y) as f32;
        let sens = if e.modifiers().shift() {
            0.0012
        } else {
            0.0055
        };
        let v = (d.start + dy * sens).clamp(0.0, 1.0);
        d.onchange.call(quantize(v, d.step));
    }
}
