//! Offline render of the demo pattern to a 16-bit WAV, for listening tests
//! and benchmarking: `cargo run -p rustbelt-engine --release --example render -- out.wav [secs] [var]`

use rustbelt_engine::{seq::demo_pattern, Command, Engine};
use std::io::Write;

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let path = args
        .get(1)
        .cloned()
        .unwrap_or_else(|| "rustbelt-demo.wav".into());
    let secs: f32 = args.get(2).and_then(|s| s.parse().ok()).unwrap_or(16.0);
    let var: usize = args.get(3).and_then(|s| s.parse().ok()).unwrap_or(0);
    let sr = 48000u32;
    let mut e = Engine::new(sr as f32);
    e.load_pattern(demo_pattern());
    e.handle(Command::QueueVar(var));
    e.handle(Command::Play);
    let n = (secs * sr as f32) as usize;
    let (mut l, mut r) = (vec![0.0f32; n], vec![0.0f32; n]);
    let t = std::time::Instant::now();
    let (mut lim, mut duck, mut blocks) = (0.0f32, 0.0f32, 0);
    for (cl, cr) in l.chunks_mut(128).zip(r.chunks_mut(128)) {
        e.render(cl, cr);
        let s = e.status();
        lim += s.lim_gr;
        duck += s.duck_gr;
        blocks += 1;
    }
    eprintln!(
        "avg limiter GR (peak-held) {:.1} dB, avg ducker GR {:.1} dB",
        lim / blocks as f32,
        duck / blocks as f32
    );
    let el = t.elapsed().as_secs_f32();
    eprintln!("rendered {secs}s in {el:.3}s ({:.1}x realtime)", secs / el);
    let mut f = std::io::BufWriter::new(std::fs::File::create(&path).unwrap());
    let data_len = (n * 4) as u32;
    f.write_all(b"RIFF").unwrap();
    f.write_all(&(36 + data_len).to_le_bytes()).unwrap();
    f.write_all(b"WAVEfmt ").unwrap();
    {
        let v = 16u32;
        f.write_all(&v.to_le_bytes()).unwrap();
    }
    for v in [1u16, 2] {
        f.write_all(&v.to_le_bytes()).unwrap();
    }
    for v in [sr, sr * 4] {
        f.write_all(&v.to_le_bytes()).unwrap();
    }
    for v in [4u16, 16] {
        f.write_all(&v.to_le_bytes()).unwrap();
    }
    f.write_all(b"data").unwrap();
    f.write_all(&data_len.to_le_bytes()).unwrap();
    for i in 0..n {
        for x in [l[i], r[i]] {
            f.write_all(&((x.clamp(-1.0, 1.0) * 32767.0) as i16).to_le_bytes())
                .unwrap();
        }
    }
    eprintln!("wrote {path}");
}
