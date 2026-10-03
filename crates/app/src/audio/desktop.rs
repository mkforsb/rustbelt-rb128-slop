//! Desktop backend: the engine renders on a dedicated thread and writes to
//! PulseAudio (works with PipeWire's pulse server too). If no server is
//! reachable it keeps running against a wall-clock "null sink" so the
//! sequencer and UI still behave.

use super::Backend;
use futures_channel::mpsc::UnboundedSender;
use libpulse_binding as pulse;
use libpulse_simple_binding::Simple;
use rustbelt_engine::{Command, Engine, Status};
use std::sync::mpsc::{channel, Receiver, Sender, TryRecvError};
use std::time::{Duration, Instant};

const SR: u32 = 48_000;
const FRAMES: usize = 256;
const STATUS_EVERY: usize = 1536;

struct DesktopBackend {
    tx: Sender<Command>,
}

impl Backend for DesktopBackend {
    fn send(&self, cmds: &[Command]) {
        for c in cmds {
            let _ = self.tx.send(*c);
        }
    }
}

pub fn start(status: UnboundedSender<Status>) -> Box<dyn Backend> {
    let (tx, rx) = channel();
    std::thread::Builder::new()
        .name("rustbelt-audio".into())
        .spawn(move || run(rx, status))
        .expect("spawn audio thread");
    Box::new(DesktopBackend { tx })
}

fn open_pulse() -> Result<Simple, pulse::error::PAErr> {
    let spec = pulse::sample::Spec {
        format: pulse::sample::Format::FLOAT32NE,
        channels: 2,
        rate: SR,
    };
    let frame_bytes = 8u32;
    let attr = pulse::def::BufferAttr {
        maxlength: u32::MAX,
        tlength: FRAMES as u32 * 4 * frame_bytes,
        prebuf: u32::MAX,
        minreq: FRAMES as u32 * frame_bytes,
        fragsize: u32::MAX,
    };
    Simple::new(
        None,
        "RUSTBELT RB-128",
        pulse::stream::Direction::Playback,
        None,
        "Master out",
        &spec,
        None,
        Some(&attr),
    )
}

fn run(rx: Receiver<Command>, status: UnboundedSender<Status>) {
    let sink = match open_pulse() {
        Ok(s) => Some(s),
        Err(e) => {
            eprintln!("RUSTBELT: PulseAudio unavailable ({e}); running silently");
            None
        }
    };
    let mut engine = Engine::new(SR as f32);
    let (mut l, mut r) = (vec![0.0f32; FRAMES], vec![0.0f32; FRAMES]);
    let mut bytes = vec![0u8; FRAMES * 8];
    let mut since_status = 0;
    let start = Instant::now();
    let mut rendered: u64 = 0;
    loop {
        loop {
            match rx.try_recv() {
                Ok(c) => engine.handle(c),
                Err(TryRecvError::Empty) => break,
                Err(TryRecvError::Disconnected) => return,
            }
        }
        engine.render(&mut l, &mut r);
        rendered += FRAMES as u64;
        since_status += FRAMES;
        if since_status >= STATUS_EVERY {
            since_status = 0;
            if status.unbounded_send(engine.status()).is_err() {
                return;
            }
        }
        match &sink {
            Some(s) => {
                for i in 0..FRAMES {
                    bytes[i * 8..i * 8 + 4].copy_from_slice(&l[i].to_ne_bytes());
                    bytes[i * 8 + 4..i * 8 + 8].copy_from_slice(&r[i].to_ne_bytes());
                }
                if let Err(e) = s.write(&bytes) {
                    eprintln!("RUSTBELT: PulseAudio write failed: {e}");
                    return;
                }
            }
            None => {
                let due = Duration::from_secs_f64(rendered as f64 / SR as f64);
                if let Some(wait) = due.checked_sub(start.elapsed()) {
                    std::thread::sleep(wait);
                }
            }
        }
    }
}
