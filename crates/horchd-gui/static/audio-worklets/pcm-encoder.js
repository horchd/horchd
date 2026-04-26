// AudioWorkletProcessor that buffers mic audio, converts float32 → int16 PCM
// and posts each frame back to the main thread alongside its peak energy.
//
// Owner constructs the AudioContext at sampleRate: 16000 so the worklet
// output matches what openWakeWord expects (no resampling here).
//
// Message shape:
//   { pcm: ArrayBuffer (int16 LE, frameSamples * 2 bytes),
//     peak: number (max |sample| in the frame, 0..1),
//     ts:   number (currentTime in ms at frame completion) }

class PcmEncoder extends AudioWorkletProcessor {
  constructor(opts) {
    super();
    const o = (opts && opts.processorOptions) || {};
    this.frameSamples = o.frameSamples || 1280; // 80 ms @ 16 kHz
    this.buf = new Int16Array(this.frameSamples);
    this.peak = 0;
    this.fill = 0;
  }

  process(inputs) {
    const input = inputs[0];
    if (!input || !input[0]) return true;
    const ch = input[0];

    for (let i = 0; i < ch.length; i++) {
      let s = ch[i];
      if (s > 1) s = 1;
      else if (s < -1) s = -1;
      const a = s < 0 ? -s : s;
      if (a > this.peak) this.peak = a;
      this.buf[this.fill++] = s < 0 ? s * 0x8000 : s * 0x7fff;
      if (this.fill === this.frameSamples) {
        const pcm = this.buf.buffer.slice(0);
        this.port.postMessage(
          { pcm, peak: this.peak, ts: currentTime * 1000 },
          [pcm],
        );
        this.buf = new Int16Array(this.frameSamples);
        this.peak = 0;
        this.fill = 0;
      }
    }
    return true;
  }
}

registerProcessor('pcm-encoder', PcmEncoder);
