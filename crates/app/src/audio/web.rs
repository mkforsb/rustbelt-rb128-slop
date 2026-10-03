//! Web backend: the engine runs inside an AudioWorklet as a separate,
//! dependency-free wasm module; this side only posts commands and receives
//! status snapshots.

use super::Backend;
use futures_channel::mpsc::UnboundedSender;
use rustbelt_engine::{Command, Status};
use wasm_bindgen::prelude::*;

static ENGINE_WASM: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/rustbelt_engine.wasm"));
static WORKLET_JS: &str = include_str!("worklet.js");

#[wasm_bindgen(inline_js = r#"
let ctx = null, node = null;
const pending = [];
export function rb_bridge_start(wasm, src, onStatus) {
  const bytes = wasm.slice();
  const AC = window.AudioContext || window.webkitAudioContext;
  ctx = new AC({ latencyHint: 'interactive' });
  const url = URL.createObjectURL(new Blob([src], { type: 'text/javascript' }));
  ctx.audioWorklet.addModule(url).then(() => {
    node = new AudioWorkletNode(ctx, 'rustbelt', {
      numberOfInputs: 0, numberOfOutputs: 1, outputChannelCount: [2],
    });
    node.port.onmessage = (e) => {
      const d = e.data;
      if (d instanceof Float32Array) onStatus(d);
      else if (d && d.type === 'error') console.error('RUSTBELT engine:', d.message);
      else if (d && d.type === 'ready') console.info('RUSTBELT engine ready @', d.sampleRate, 'Hz');
    };
    node.connect(ctx.destination);
    node.port.postMessage({ type: 'init', wasm: bytes }, [bytes.buffer]);
    for (const m of pending) node.port.postMessage(m, [m.data.buffer]);
    pending.length = 0;
  }).catch((err) => console.error('RUSTBELT worklet failed:', err));
  const resume = () => { if (ctx && ctx.state !== 'running') ctx.resume(); };
  for (const ev of ['pointerdown', 'keydown', 'touchend']) window.addEventListener(ev, resume, true);
}
export function rb_bridge_send(data) {
  const m = { type: 'cmds', data: data.slice() };
  if (node) node.port.postMessage(m, [m.data.buffer]); else pending.push(m);
}
export function rb_bridge_running() { return !!ctx && ctx.state === 'running'; }
export function rb_bridge_resume() { if (ctx && ctx.state !== 'running') ctx.resume(); }
"#)]
extern "C" {
    fn rb_bridge_start(wasm: &[u8], src: &str, on_status: &js_sys::Function);
    fn rb_bridge_send(data: &[f64]);
    fn rb_bridge_running() -> bool;
    fn rb_bridge_resume();
}

struct WebBackend;

impl Backend for WebBackend {
    fn send(&self, cmds: &[Command]) {
        let data: Vec<f64> = cmds.iter().flat_map(|c| c.encode()).collect();
        rb_bridge_send(&data);
    }
    fn running(&self) -> bool {
        rb_bridge_running()
    }
    fn resume(&self) {
        rb_bridge_resume();
    }
}

pub fn start(tx: UnboundedSender<Status>) -> Box<dyn Backend> {
    let cb = Closure::<dyn FnMut(js_sys::Float32Array)>::new(move |a: js_sys::Float32Array| {
        if let Some(s) = Status::read(&a.to_vec()) {
            let _ = tx.unbounded_send(s);
        }
    });
    rb_bridge_start(ENGINE_WASM, WORKLET_JS, cb.as_ref().unchecked_ref());
    cb.forget();
    Box::new(WebBackend)
}
