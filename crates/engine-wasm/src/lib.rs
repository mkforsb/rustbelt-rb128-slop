//! C-ABI wrapper around the engine for use inside an `AudioWorkletProcessor`.
//!
//! No wasm-bindgen here on purpose: the worklet global scope cannot load
//! wasm-bindgen's JS glue, so the module exposes plain functions and shares
//! audio and status buffers through linear memory.

use rustbelt_engine::{Command, Engine, STATUS_LEN};
use std::cell::UnsafeCell;

const MAX_FRAMES: usize = 4096;

struct State {
    engine: Option<Engine>,
    out_l: [f32; MAX_FRAMES],
    out_r: [f32; MAX_FRAMES],
    status: [f32; STATUS_LEN],
    cmd: [f64; 8],
}

struct Global(UnsafeCell<State>);
// SAFETY: the worklet runs on a single thread and calls in sequentially.
unsafe impl Sync for Global {}

static STATE: Global = Global(UnsafeCell::new(State {
    engine: None,
    out_l: [0.0; MAX_FRAMES],
    out_r: [0.0; MAX_FRAMES],
    status: [0.0; STATUS_LEN],
    cmd: [0.0; 8],
}));

fn state() -> &'static mut State {
    // SAFETY: see `Global`; no reentrancy is possible from JS.
    unsafe { &mut *STATE.0.get() }
}

#[no_mangle]
pub extern "C" fn rb_init(sample_rate: f32) {
    state().engine = Some(Engine::new(sample_rate));
}

/// Pointer to an 8-slot f64 scratch area; JS fills it and calls `rb_cmd`.
#[no_mangle]
pub extern "C" fn rb_cmd_ptr() -> *mut f64 {
    state().cmd.as_mut_ptr()
}

#[no_mangle]
pub extern "C" fn rb_cmd() {
    let s = state();
    if let (Some(e), Some(c)) = (s.engine.as_mut(), Command::decode(&s.cmd)) {
        e.handle(c);
    }
}

#[no_mangle]
pub extern "C" fn rb_render(frames: u32) {
    let s = state();
    let n = (frames as usize).min(MAX_FRAMES);
    match s.engine.as_mut() {
        Some(e) => e.render(&mut s.out_l[..n], &mut s.out_r[..n]),
        None => {
            s.out_l[..n].fill(0.0);
            s.out_r[..n].fill(0.0);
        }
    }
}

#[no_mangle]
pub extern "C" fn rb_out_l() -> *const f32 {
    state().out_l.as_ptr()
}

#[no_mangle]
pub extern "C" fn rb_out_r() -> *const f32 {
    state().out_r.as_ptr()
}

/// Refreshes and returns the status snapshot (`rb_status_len` floats).
#[no_mangle]
pub extern "C" fn rb_status() -> *const f32 {
    let s = state();
    if let Some(e) = s.engine.as_ref() {
        e.status().write(&mut s.status);
    }
    s.status.as_ptr()
}

#[no_mangle]
pub extern "C" fn rb_status_len() -> u32 {
    STATUS_LEN as u32
}
