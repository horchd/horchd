// 16 kHz mono int16 PCM capture via WebAudio + AudioWorklet.
//
// Mirrors Lyna's approach: AudioContext({ sampleRate: 16000 }) feeds a
// MediaStreamSource into the pcm-encoder worklet, which posts 80 ms
// frames as ArrayBuffer chunks. Subscribers get a live `peak` reading
// for the meter and the raw PCM frames; `recordFixed` collects exactly
// `durationMs` worth of samples and returns them as one Int16Array.

const SAMPLE_RATE = 16_000;
const FRAME_SAMPLES = 1280; // 80 ms @ 16 kHz

export type PcmFrame = { pcm: Int16Array; peak: number; tsMs: number };

export type Recorder = {
  /** Live mic peak in [0, 1]. Updates ~12.5 Hz. */
  onFrame: (cb: (f: PcmFrame) => void) => () => void;
  /** Capture exactly durationMs of audio. Resolves with a single int16 PCM blob at 16 kHz mono. */
  recordFixed: (durationMs: number) => Promise<Int16Array>;
  /** Tear down the AudioContext + mic stream. */
  close: () => Promise<void>;
  /** Whether the AudioContext actually negotiated 16 kHz. */
  actualSampleRate: number;
};

export async function openRecorder(): Promise<Recorder> {
  if (!navigator.mediaDevices?.getUserMedia) {
    throw new Error("getUserMedia is not available");
  }
  const stream = await navigator.mediaDevices.getUserMedia({
    audio: {
      channelCount: 1,
      sampleRate: SAMPLE_RATE,
      echoCancellation: false,
      noiseSuppression: false,
      autoGainControl: false,
    },
  });

  let ac: AudioContext;
  try {
    ac = new AudioContext({ sampleRate: SAMPLE_RATE, latencyHint: "interactive" });
  } catch {
    ac = new AudioContext({ latencyHint: "interactive" });
  }
  await ac.audioWorklet.addModule("/audio-worklets/pcm-encoder.js");

  const node = new AudioWorkletNode(ac, "pcm-encoder", {
    processorOptions: { frameSamples: FRAME_SAMPLES },
  });
  ac.createMediaStreamSource(stream).connect(node);

  const subs = new Set<(f: PcmFrame) => void>();
  node.port.onmessage = (e) => {
    const { pcm, peak, ts } = e.data as { pcm: ArrayBuffer; peak: number; ts: number };
    const samples = new Int16Array(pcm);
    const f: PcmFrame = { pcm: samples, peak, tsMs: ts };
    for (const cb of subs) cb(f);
  };

  function onFrame(cb: (f: PcmFrame) => void): () => void {
    subs.add(cb);
    return () => {
      subs.delete(cb);
    };
  }

  function recordFixed(durationMs: number): Promise<Int16Array> {
    const wantSamples = Math.round((durationMs / 1000) * SAMPLE_RATE);
    const out = new Int16Array(wantSamples);
    let written = 0;
    return new Promise<Int16Array>((resolve) => {
      const unsub = onFrame((f) => {
        if (written >= wantSamples) return;
        const remaining = wantSamples - written;
        const take = Math.min(remaining, f.pcm.length);
        out.set(f.pcm.subarray(0, take), written);
        written += take;
        if (written >= wantSamples) {
          unsub();
          resolve(out);
        }
      });
    });
  }

  async function close(): Promise<void> {
    subs.clear();
    try {
      node.disconnect();
    } catch {
      /* noop */
    }
    stream.getTracks().forEach((t) => t.stop());
    await ac.close().catch(() => undefined);
  }

  return {
    actualSampleRate: ac.sampleRate,
    onFrame,
    recordFixed,
    close,
  };
}

export const PCM_SAMPLE_RATE = SAMPLE_RATE;
