# RUSTBELT RB-128

A techno session groovebox written in Rust with [Dioxus](https://dioxuslabs.com).
The same code runs in the browser (WebAudio, via an AudioWorklet) and on the desktop (PulseAudio).

> Detroit, the birthplace of techno, sits in the Rust Belt. RB-128 = Rust Belt, 128 steps.

## Run it

```sh
# Desktop (PulseAudio / PipeWire-pulse). Needs libpulse and webkit2gtk-4.1 dev packages.
cargo run --release

# Web (WebAudio). Run dx from the workspace root (dx 0.7 resolves
# `default-members` relative to the current directory).
dx serve --platform web
dx bundle --platform web --release     # static files in target/dx/rustbelt/release/web/public
```

The browser starts audio on your first click or key press, because of autoplay rules.

## What's in the box

| Channel | Synthesis | Sound controls |
|---|---|---|
| **Kick** | sine body with pitch envelope, separate sub oscillator, non-tonal hit (band-passed noise and inharmonic partials), distortion | Tune, Bend, Bend T, Decay, Hit, Hit Tone, Sub, Sub Dec, Drive |
| ↳ **Rumble** | a dedicated 4-line FDN reverb fed by the kick → drive → 24 dB/oct low cut and high cut → **ducked by the kick's own envelope** | Rumble, Rmb Dec, Rmb Lo, Rmb Hi, Rmb Drv, Rmb Duck |
| Closed / Open hat | six square waves at 808 metallic ratios, plus noise, band-pass and high-pass filtering; the closed hat chokes the open hat | Pitch, Attack, Decay, Tone, Noise |
| Ride | metallic oscillators at ride ratios plus a bell partial | + Bell |
| Snare | two tuned bodies with a pitch bend, plus filtered "snappy" noise | Tune, Bend, Decay, Snappy, Snap Dec, Snap Tone |
| Clap | band-passed noise in 1–6 bursts, then a tail | Tone, Reso, Spread, Bursts, Decay |
| Low / Mid / High tom | sine with a pitch bend and a noise click | Tune, Bend, Bend T, Decay, Noise |
| Low / Mid / High conga | the same voice tuned higher, with a resonant slap | Tune, Bend, Bend T, Decay, Slap |
| **FM stab** | 3 freely tuned voices (±2 octaves each in 0.1-semitone steps; wheel ±1 st, shift+wheel ±0.1 st), each a 2-operator FM pair with feedback, then a 4-pole low-pass | Tune, Voice 1–3, Ratio, FM Amt, FM Dec, Fdbk, Attack, Decay, Cutoff, Detune |

**Every channel has:** randomize ⚄ (each parameter is randomized within a range that keeps it musical), mute and solo, which stop *notes* rather than audio so rumble, reverb and decays ring out (keys **1 2 3 4 5 6 7 8 9 0 Q W E** mute tracks 1–13 by physical key position), Lo/Mid/Hi EQ (±15 dB), post-fader delay and reverb sends, pan and level, and a level meter.

**Master section:**
- Tempo-synced ping-pong delay: 1/32 to 1/2 including triplet and dotted times, feedback, hi cut, lo cut, ping-pong width, return.
- 8-line FDN reverb with diffusers, pre-delay and low cut.
- Sidechain **ducker**, wired automatically: the dry kick is the key, and it ducks everything else including the effect returns. Threshold, ratio, attack, release and a GR meter. The kick and its rumble are not ducked.
- **DJ filter** on one knob. Centre is exactly transparent (the state-variable filter's LP + k·BP + HP sums to the input). Turning left passes through a band-emphasised response into a resonant low-pass sweeping down; turning right does the same into a high-pass sweeping up. The Reso knob sets how much resonance builds up as the filter closes.
- Look-ahead **limiter** with makeup gain, ceiling, release and a GR meter, then the master volume.

**Transport:** play/pause (space), stop/rewind, BPM 60–200 (± buttons or the Tempo knob), and swing from 50 to 75 %.

## Sequencer

Like a Roland TR, one row of 16 step buttons is shared by every track: pick a track, then program it.

- **Pages:** each track is 1–8 pages of 16 steps (up to 128), set with **LENGTH − / +**. **FOLLOW** keeps the visible page on the playhead.
- **Polyrhythm:** press **LOOP**, then press any step. That step becomes the track's loop point (for example 3, 5 or 7 steps against a 16-step kick). Loop-point tracks show ∿ in the track row.
- **Per-step lanes:** velocity, free pitch (±2 octaves in 0.1-semitone steps, e.g. +12.4; hold ctrl while drawing to snap to semitones) and probability. Draw curves by dragging across the lane, and skipped columns are interpolated. **Shift-drag** fine-adjusts one step at 1/10 speed, the **mouse wheel** nudges a step (±1 st, or ±0.1 st with shift), and double-click resets a step's value in the current lane.
- **Copy/paste:** COPY PG and COPY TRK fill the clipboard, and PASTE writes into the current page or track, including across tracks and variations. **×2** duplicates the loop (16→32→64→128). CLR clears the page, and ◂ ▸ rotate the track.
- **Variations A/B/C:** each holds its own 16–128-step sequence for every track. Picking one *cues* it; the cued button blinks. The switch happens in tempo, at the end of the current variation's longest loop, and every track then restarts from step 1. While stopped, the switch is immediate. The variation you picked last is the one you edit.
- **Random:** STEPS writes a new instrument-aware rhythm on this page (the kick favours downbeats, the open hat favours off-beats, and so on). LANE randomizes the shown lane, MUTATE flips a few steps, and ALL TRACKS writes a new rhythm for the whole variation.

## Architecture

```
crates/
  engine/        pure-Rust DSP, sequencer, pattern model, command protocol (no dependencies)
  engine-wasm/   C-ABI wasm wrapper of the engine (no wasm-bindgen) for the AudioWorklet
  app/           Dioxus UI + audio backends
    build.rs     web target only: builds engine-wasm and embeds it via include_bytes!
    src/audio/   web.rs (AudioWorklet bridge), desktop.rs (PulseAudio thread), worklet.js
```

- The UI owns the state and mirrors every edit to the engine as a small `Command`. The wire format is 8 × f64, which is easy to post to a worklet.
- The engine runs **inside the audio thread** on both platforms: in an `AudioWorkletProcessor` on the web, and on a dedicated thread writing to PulseAudio on the desktop. So timing is sample-accurate, including swing and probability, and the UI being busy never makes the beat drift.
- The engine reports back a `Status` snapshot about 30 times per second: playhead per track, the playing and cued variation, meters and gain reduction.
- If no PulseAudio server is reachable, the desktop build keeps running against a wall-clock null sink so the UI still works.

## Testing

```sh
cargo test -p rustbelt-engine --release      # protocol, mapping, every voice sounds and decays,
                                             # variation cueing, polyrhythm, DJ filter transparency
cargo run -p rustbelt-engine --release --example render -- demo.wav 16 0   # offline render of variation A
```
