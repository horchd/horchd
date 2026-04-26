<script lang="ts">
  import { Mic, Square, Trash2, Hammer } from "@lucide/svelte";

  import { app } from "$lib/app.svelte";
  import { dbus } from "$lib/dbus";
  import type { SampleKind, TrainingSample, TrainingWord } from "$lib/types";

  let words = $state<TrainingWord[]>([]);
  let activeName = $state<string>("");
  let nameInput = $state<string>("");
  let samples = $state<TrainingSample[]>([]);
  let trainingDir = $state<string>("…");

  let recording = $state<SampleKind | undefined>(undefined);
  let recorder = $state<MediaRecorder | undefined>(undefined);
  let micStream = $state<MediaStream | undefined>(undefined);
  let recordError = $state<string | undefined>(undefined);

  $effect(() => {
    void refreshDir();
    void refreshWords();
    return () => stopMic();
  });

  $effect(() => {
    void refreshSamples(activeName);
  });

  async function refreshDir() {
    try {
      trainingDir = await dbus.trainingDir();
    } catch {
      trainingDir = "(unavailable)";
    }
  }

  async function refreshWords() {
    try {
      words = await dbus.listTrainingWords();
      if (!activeName && words.length > 0) {
        activeName = words[0].name;
      }
    } catch (e) {
      app.showToast(`load words: ${formatErr(e)}`, true);
    }
  }

  async function refreshSamples(name: string) {
    if (!name) {
      samples = [];
      return;
    }
    try {
      samples = await dbus.listTrainingSamples(name);
    } catch (e) {
      app.showToast(`load samples: ${formatErr(e)}`, true);
    }
  }

  function selectWord(name: string) {
    activeName = name;
    recordError = undefined;
  }

  function createWord() {
    const trimmed = nameInput.trim();
    if (!trimmed) return;
    if (!/^[A-Za-z0-9_-]+$/.test(trimmed)) {
      app.showToast("name must be ASCII letters/digits/_-", true);
      return;
    }
    activeName = trimmed;
    nameInput = "";
    if (!words.find((w) => w.name === trimmed)) {
      words = [...words, { name: trimmed, positive: 0, negative: 0 }].sort(
        (a, b) => a.name.localeCompare(b.name),
      );
    }
  }

  async function ensureMic(): Promise<MediaStream> {
    if (micStream) return micStream;
    const s = await navigator.mediaDevices.getUserMedia({
      audio: {
        channelCount: 1,
        echoCancellation: false,
        noiseSuppression: false,
        autoGainControl: false,
      },
    });
    micStream = s;
    return s;
  }

  function stopMic() {
    micStream?.getTracks().forEach((t) => t.stop());
    micStream = undefined;
  }

  function pickMime(): string {
    const cands = [
      "audio/webm;codecs=opus",
      "audio/webm",
      "audio/ogg;codecs=opus",
      "audio/ogg",
    ];
    for (const m of cands) {
      if (typeof MediaRecorder !== "undefined" && MediaRecorder.isTypeSupported(m)) return m;
    }
    return "audio/webm";
  }

  async function startRecording(kind: SampleKind) {
    if (!activeName) {
      recordError = "pick or create a word first";
      return;
    }
    if (recording) return;
    recordError = undefined;
    try {
      const stream = await ensureMic();
      const mime = pickMime();
      const mr = new MediaRecorder(stream, { mimeType: mime });
      const chunks: BlobPart[] = [];
      mr.addEventListener("dataavailable", (e) => {
        if (e.data && e.data.size > 0) chunks.push(e.data);
      });
      mr.addEventListener("stop", () => {
        void persistRecording(kind, mime, chunks);
      });
      recorder = mr;
      recording = kind;
      mr.start();
    } catch (e) {
      recordError = formatErr(e);
      recording = undefined;
      recorder = undefined;
    }
  }

  function stopRecording() {
    if (!recorder) return;
    if (recorder.state !== "inactive") recorder.stop();
  }

  async function persistRecording(kind: SampleKind, mime: string, chunks: BlobPart[]) {
    recording = undefined;
    recorder = undefined;
    if (chunks.length === 0) return;
    try {
      const blob = new Blob(chunks, { type: mime });
      const buf = new Uint8Array(await blob.arrayBuffer());
      const saved = await dbus.saveTrainingSample(activeName, kind, mime, buf);
      app.showToast(`saved ${kind} (${(saved.size / 1024).toFixed(1)} KB)`);
      await Promise.all([refreshSamples(activeName), refreshWords()]);
    } catch (e) {
      recordError = formatErr(e);
    }
  }

  async function removeSample(path: string) {
    try {
      await dbus.deleteTrainingSample(path);
      await Promise.all([refreshSamples(activeName), refreshWords()]);
    } catch (e) {
      app.showToast(`delete failed: ${formatErr(e)}`, true);
    }
  }

  async function tryTrain() {
    try {
      await dbus.trainWakeword(activeName);
      await refreshWords();
    } catch (e) {
      app.showToast(formatErr(e), true);
    }
  }

  function formatErr(e: unknown): string {
    if (typeof e === "string") return e;
    if (e instanceof Error) return e.message;
    return String(e);
  }

  function fmtTime(ts: number): string {
    if (!ts) return "—";
    const d = new Date(ts);
    return `${d.toLocaleDateString()} ${d.toLocaleTimeString()}`;
  }

  function fmtSize(n: number): string {
    if (n < 1024) return `${n} B`;
    if (n < 1024 * 1024) return `${(n / 1024).toFixed(1)} KB`;
    return `${(n / 1024 / 1024).toFixed(2)} MB`;
  }

  const positiveCount = $derived(samples.filter((s) => s.kind === "positive").length);
  const negativeCount = $derived(samples.filter((s) => s.kind === "negative").length);
</script>

<section class="pt-7">
  <header class="mb-6">
    <h2 class="label-tracked text-ink font-bold m-0">Train a wakeword</h2>
    <p class="text-muted text-[11px] mt-1.5 leading-[1.55]">
      Record positive (you saying it) and negative (random speech / noise)
      samples. Files are stored under
      <code class="font-mono bg-paper-2 px-1.5 py-px text-[11px]">{trainingDir}</code>.
      The training engine itself is not wired up yet — for production
      wakewords use openWakeWord and import the resulting <code class="font-mono bg-paper-2 px-1.5 py-px text-[11px]">.onnx</code>.
    </p>
  </header>

  <div class="mb-6 pb-5 border-b border-rule-soft">
    <header class="flex items-baseline justify-between mb-2.5">
      <span class="label-tracked text-ink font-bold">Wakeword</span>
      <span class="text-muted text-[10px]">{words.length} on disk</span>
    </header>

    <div class="flex flex-wrap gap-1.5 mb-3">
      {#each words as w (w.name)}
        <button
          type="button"
          class="inline-flex items-center gap-1.5 font-mono text-[11px] px-2.5 py-1 border cursor-pointer transition-colors"
          class:bg-ink={activeName === w.name}
          class:text-paper={activeName === w.name}
          class:border-ink={activeName === w.name}
          class:bg-transparent={activeName !== w.name}
          class:text-ink={activeName !== w.name}
          class:border-rule-soft={activeName !== w.name}
          onclick={() => selectWord(w.name)}
        >
          {w.name}
          <span class="text-[9px] tracking-[0.1em] opacity-70">
            {w.positive}+ / {w.negative}−
          </span>
        </button>
      {/each}
      {#if words.length === 0}
        <span class="text-muted font-mono text-[11px] italic">no wakewords yet — create one ↓</span>
      {/if}
    </div>

    <div class="grid grid-cols-[1fr_auto] gap-2">
      <input
        class="bg-paper-2 border border-rule px-3 py-2 font-mono text-[12px] text-ink min-w-0
               focus:outline-2 focus:outline-accent focus:-outline-offset-1"
        type="text"
        placeholder="new word (e.g. wetter, jarvis)"
        bind:value={nameInput}
        onkeydown={(e) => {
          if (e.key === "Enter") {
            e.preventDefault();
            createWord();
          }
        }}
      />
      <button
        class="font-mono font-semibold text-[10px] tracking-[0.2em] uppercase px-3 py-2 border border-ink bg-ink text-paper cursor-pointer transition-colors hover:bg-accent hover:border-accent"
        onclick={createWord}
        disabled={!nameInput.trim()}
      >Create</button>
    </div>
  </div>

  {#if activeName}
    <div class="mb-6 pb-5 border-b border-rule-soft">
      <header class="flex items-baseline justify-between mb-2.5">
        <span class="label-tracked text-ink font-bold">
          Record samples for <code class="font-mono text-accent">{activeName}</code>
        </span>
        <span class="text-muted text-[10px]">
          {positiveCount} positive · {negativeCount} negative
        </span>
      </header>

      <div class="grid grid-cols-2 gap-3 mb-3">
        {#each ["positive", "negative"] as kind (kind)}
          {@const k = kind as SampleKind}
          {@const isMine = recording === k}
          {@const otherActive = recording !== undefined && !isMine}
          <button
            class="flex flex-col items-center justify-center gap-2 px-4 py-5 border-2 cursor-pointer transition-colors disabled:opacity-40 disabled:cursor-not-allowed"
            class:border-ink={!isMine}
            class:border-accent={isMine}
            class:bg-paper-2={!isMine}
            class:bg-[color-mix(in_oklab,var(--color-accent)_15%,var(--color-paper))]={isMine}
            onclick={() => (isMine ? stopRecording() : startRecording(k))}
            disabled={otherActive}
          >
            {#if isMine}
              <Square size="22" strokeWidth={2.2} class="text-accent" />
              <span class="font-mono font-semibold text-[11px] tracking-[0.18em] uppercase text-accent">
                Stop {k}
              </span>
              <span class="font-mono text-[10px] text-accent animate-pulse">● recording…</span>
            {:else}
              <Mic size="22" strokeWidth={2.2} />
              <span class="font-mono font-semibold text-[11px] tracking-[0.18em] uppercase">
                Record {k}
              </span>
              <span class="font-mono text-[10px] text-muted">
                {k === "positive" ? "say the word" : "speak / noise"}
              </span>
            {/if}
          </button>
        {/each}
      </div>

      {#if recordError}
        <div class="text-accent text-[12px] px-2.5 py-2 border border-accent
                    bg-[color-mix(in_oklab,var(--color-accent)_8%,var(--color-paper))] mb-3">
          {recordError}
        </div>
      {/if}

      <p class="m-0 text-[10px] leading-[1.6] text-muted">
        Tip: 50–100 positives at varied tone/distance and a couple of
        hundred negatives drawn from your typical room noise is a
        reasonable starting point.
      </p>
    </div>

    <div class="mb-6">
      <header class="flex items-baseline justify-between mb-2.5">
        <span class="label-tracked text-ink font-bold">Samples</span>
        {#if samples.length > 0}
          <span class="text-muted text-[10px]">{samples.length} total</span>
        {/if}
      </header>

      {#if samples.length === 0}
        <div class="px-4 py-6 text-center text-muted border border-rule-soft font-display italic text-[14px]">
          No recordings yet for <code class="font-mono not-italic text-[12px]">{activeName}</code>.
        </div>
      {:else}
        <div class="border border-rule-soft divide-y divide-[color:var(--color-rule-soft)]">
          {#each samples as s (s.path)}
            <div class="grid grid-cols-[auto_1fr_auto_auto] items-center gap-3 px-3 py-2">
              <span
                class="px-1.5 py-px font-mono font-semibold text-[9px] tracking-[0.18em] uppercase border"
                class:border-ok={s.kind === "positive"}
                class:text-ok={s.kind === "positive"}
                class:border-accent={s.kind === "negative"}
                class:text-accent={s.kind === "negative"}
              >
                {s.kind === "positive" ? "POS" : "NEG"}
              </span>
              <span class="font-mono text-[11px] text-ink whitespace-nowrap overflow-hidden text-ellipsis">
                {fmtTime(s.ts_ms)}
              </span>
              <span class="font-mono text-[10px] text-muted">{fmtSize(s.size)}</span>
              <button
                class="inline-flex items-center justify-center w-7 h-7 border border-rule-soft bg-transparent text-muted cursor-pointer transition-colors hover:bg-accent hover:border-accent hover:text-paper"
                onclick={() => removeSample(s.path)}
                aria-label="Delete sample"
                title="Delete sample"
              >
                <Trash2 size="13" strokeWidth={2.2} />
              </button>
            </div>
          {/each}
        </div>
      {/if}
    </div>

    <div>
      <button
        class="inline-flex items-center gap-2 font-mono font-semibold text-[11px] tracking-[0.18em] uppercase px-4 py-2.5 border border-ink bg-ink text-paper cursor-pointer transition-colors hover:bg-accent hover:border-accent disabled:opacity-50 disabled:cursor-not-allowed"
        onclick={tryTrain}
        disabled={positiveCount === 0 || negativeCount === 0}
      >
        <Hammer size="14" strokeWidth={2.2} />
        Train {activeName}
      </button>
      <p class="m-0 text-[10px] leading-[1.6] text-muted mt-2">
        The training pipeline (mel + embedding feature extraction → small
        classifier head trained on your samples → ONNX export) is on the
        roadmap. Today this button reports its status.
      </p>
    </div>
  {/if}
</section>
