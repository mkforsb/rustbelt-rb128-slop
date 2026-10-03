// AudioWorkletProcessor hosting the RUSTBELT engine (raw C-ABI wasm module).
// Loaded from a Blob URL created by the main-thread bridge.
const STATUS_EVERY = 1536; // frames between status posts (~32 ms at 48 kHz)

class RustbeltProcessor extends AudioWorkletProcessor {
  constructor() {
    super();
    this.ex = null;
    this.queue = [];
    this.sinceStatus = 0;
    this.port.onmessage = (e) => this.onMessage(e.data);
  }

  onMessage(m) {
    if (m.type === 'init') {
      WebAssembly.instantiate(m.wasm, {}).then(({ instance }) => {
        const ex = instance.exports;
        ex.rb_init(sampleRate);
        this.ex = ex;
        for (const q of this.queue) this.apply(q);
        this.queue = [];
        this.port.postMessage({ type: 'ready', sampleRate });
      }).catch((err) => this.port.postMessage({ type: 'error', message: String(err) }));
    } else if (m.type === 'cmds') {
      if (this.ex) this.apply(m.data); else this.queue.push(m.data);
    }
  }

  apply(data) {
    const ex = this.ex;
    const ptr = ex.rb_cmd_ptr();
    for (let i = 0; i + 8 <= data.length; i += 8) {
      new Float64Array(ex.memory.buffer, ptr, 8).set(data.subarray(i, i + 8));
      ex.rb_cmd();
    }
  }

  process(_inputs, outputs) {
    const out = outputs[0];
    const ex = this.ex;
    if (!ex || out.length === 0) return true;
    const n = out[0].length;
    ex.rb_render(n);
    const mem = ex.memory.buffer;
    out[0].set(new Float32Array(mem, ex.rb_out_l(), n));
    if (out.length > 1) out[1].set(new Float32Array(mem, ex.rb_out_r(), n));
    this.sinceStatus += n;
    if (this.sinceStatus >= STATUS_EVERY) {
      this.sinceStatus = 0;
      const p = ex.rb_status();
      const st = new Float32Array(ex.memory.buffer, p, ex.rb_status_len()).slice();
      this.port.postMessage(st, [st.buffer]);
    }
    return true;
  }
}

registerProcessor('rustbelt', RustbeltProcessor);
