//! Audio backends. Both run the same `rustbelt_engine::Engine`; only the
//! transport differs (AudioWorklet + postMessage on the web, a PulseAudio
//! thread + channels on the desktop).

use futures_channel::mpsc::{unbounded, UnboundedReceiver, UnboundedSender};
use rustbelt_engine::{Command, Status};
use std::cell::RefCell;

#[cfg(all(feature = "desktop", not(target_arch = "wasm32")))]
mod desktop;
#[cfg(all(feature = "web", target_arch = "wasm32"))]
mod web;

pub trait Backend {
    fn send(&self, cmds: &[Command]);
    /// False while the browser is still waiting for a user gesture.
    fn running(&self) -> bool {
        true
    }
    fn resume(&self) {}
}

struct Silent;
impl Backend for Silent {
    fn send(&self, _: &[Command]) {}
}

thread_local! {
    static BACKEND: RefCell<Box<dyn Backend>> = RefCell::new(Box::new(Silent));
}

#[allow(unreachable_code)]
fn platform_start(tx: UnboundedSender<Status>) -> Box<dyn Backend> {
    #[cfg(all(feature = "web", target_arch = "wasm32"))]
    return web::start(tx);
    #[cfg(all(feature = "desktop", not(target_arch = "wasm32")))]
    return desktop::start(tx);
    let _ = tx;
    Box::new(Silent)
}

/// Start the audio engine; status snapshots arrive on the returned stream.
pub fn start() -> UnboundedReceiver<Status> {
    let (tx, rx) = unbounded();
    let b = platform_start(tx);
    BACKEND.with(|cell| *cell.borrow_mut() = b);
    rx
}

pub fn send(cmd: Command) {
    send_all(&[cmd]);
}

pub fn send_all(cmds: &[Command]) {
    if !cmds.is_empty() {
        BACKEND.with(|b| b.borrow().send(cmds));
    }
}

pub fn running() -> bool {
    BACKEND.with(|b| b.borrow().running())
}

pub fn resume() {
    BACKEND.with(|b| b.borrow().resume());
}
