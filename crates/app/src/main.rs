//! RUSTBELT RB-128: a techno session groovebox.
//!
//! `dx serve --platform web` (WebAudio / AudioWorklet) or
//! `dx serve --platform desktop` / `cargo run` (PulseAudio).

mod audio;
mod state;
mod ui;

use dioxus::prelude::*;
use futures_util::StreamExt;
use state::App;
use ui::{
    header::Header, instrument::InstrumentPanel, knob, master::MasterPanel, mixer::Mixer,
    sequencer::Sequencer,
};

const FAVICON: &str = "data:image/svg+xml,%3Csvg xmlns='http://www.w3.org/2000/svg' viewBox='0 0 16 16'%3E%3Crect width='16' height='16' rx='3' fill='%23ff6a1f'/%3E%3Ccircle cx='8' cy='8' r='4' fill='%23121214'/%3E%3C/svg%3E";
const KEYBOARD_JS: &str = r#"
const handled = new Set(['Space', 'Digit1', 'Digit2', 'Digit3', 'Digit4', 'Digit5', 'Digit6',
  'Digit7', 'Digit8', 'Digit9', 'Digit0', 'KeyQ', 'KeyW', 'KeyE']);
window.addEventListener('keydown', (e) => {
  if (e.ctrlKey || e.altKey || e.metaKey || !handled.has(e.code)) return;
  const t = e.target;
  if (t && (t.tagName === 'INPUT' || t.tagName === 'TEXTAREA' || t.isContentEditable)) return;
  e.preventDefault();
  if (!e.repeat) dioxus.send(e.code);
}, true);
await new Promise(() => {});
"#;
const STYLE: &str = include_str!("../assets/style.css");

fn main() {
    #[cfg(all(feature = "desktop", not(target_arch = "wasm32")))]
    {
        use dioxus::desktop::{Config, LogicalSize, WindowBuilder};
        let window = WindowBuilder::new()
            .with_title("RUSTBELT RB-128")
            .with_inner_size(LogicalSize::new(1480.0, 1000.0))
            .with_min_inner_size(LogicalSize::new(900.0, 600.0));
        let cfg = Config::new()
            .with_window(window)
            .with_menu(None)
            .with_background_color((20, 20, 22, 255));
        dioxus::LaunchBuilder::desktop().with_cfg(cfg).launch(Root);
        return;
    }
    #[allow(unreachable_code)]
    dioxus::launch(Root);
}

#[component]
fn Root() -> Element {
    let app = use_hook(App::new);
    use_context_provider(|| app);
    use_hook(move || {
        let mut rx = audio::start();
        let mut running = app.audio_running;
        running.set(audio::running());
        app.sync_all();
        spawn(async move {
            let mut app = app;
            while let Some(status) = rx.next().await {
                app.apply_status(status);
            }
        });
    });

    // Window-level shortcuts (work regardless of which element has focus):
    // space = play/pause, 1 2 3 4 5 6 7 8 9 0 Q W E = mute tracks 1-13.
    use_hook(move || {
        spawn(async move {
            let mut keys = document::eval(KEYBOARD_JS);
            while let Ok(code) = keys.recv::<String>().await {
                if code == "Space" {
                    app.play_pause();
                } else if let Some(ch) = ui::mute_key_channel(&code) {
                    app.toggle_mute(ch);
                }
            }
        });
    });

    let end_gestures = move || {
        let (mut drag, mut draw, mut drawing) = (app.drag, app.draw, app.drawing);
        if *drawing.peek() {
            drawing.set(false);
        }
        if drag.peek().is_some() {
            drag.set(None);
        }
        if draw.peek().is_some() {
            draw.set(None);
        }
    };

    rsx! {
        document::Title { "RUSTBELT RB-128" }
        document::Link { rel: "icon", href: FAVICON }
        // Inlined so it also works with a plain `cargo run` (no dx asset bundling).
        style { {STYLE} }
        div {
            class: if app.drag.read().is_some() { "rb dragging" } else { "rb" },
            onpointermove: move |e| knob::drag_move(&app, &e),
            onpointerup: move |_| end_gestures(),
            onpointercancel: move |_| end_gestures(),
            onpointerleave: move |_| end_gestures(),
            Header {}
            if !*app.audio_running.read() {
                div { class: "audio-hint", onclick: move |_| audio::resume(),
                    "Audio is paused by the browser. Click anywhere or press play to start the engine."
                }
            }
            div { class: "row",
                InstrumentPanel {}
                MasterPanel {}
            }
            Mixer {}
            Sequencer {}
            footer { class: "footer",
                "drag knobs vertically · shift = fine · double-click = default · space = play/pause · 1–0, Q W E = mute tracks · "
                "variations switch when the longest loop wraps"
            }
        }
    }
}
