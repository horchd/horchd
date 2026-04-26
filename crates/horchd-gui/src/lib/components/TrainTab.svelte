<script lang="ts">
  import { Mic, Trash2, Hammer, Play, Pause, Loader2, Check, AlertCircle } from "@lucide/svelte";

  import { app } from "$lib/app.svelte";
  import { dbus, onTrain } from "$lib/dbus";
  import { openRecorder, PCM_SAMPLE_RATE, type Recorder } from "$lib/audio/recordPcm";
  import type { SampleKind, TrainingSample, TrainingWord } from "$lib/types";

  const TAKE_MS = 2000;
  const MAX_LOG_LINES = 400;

  let words = $state<TrainingWord[]>([]);
  let activeName = $state<string>("");
  let nameInput = $state<string>("");
  let phraseInput = $state<string>("");
  let samples = $state<TrainingSample[]>([]);
  let trainingDir = $state<string>("…");

  let recorder = $state<Recorder | undefined>(undefined);
  let recordingKind = $state<SampleKind | undefined>(undefined);
  let progress = $state(0);
  let micPeak = $state(0);
  let recordError = $state<string | undefined>(undefined);
  let unsubFrame: (() => void) | undefined;
  let progressTimer: ReturnType<typeof setInterval> | undefined;

  let playingPath = $state<string | undefined>(undefined);
  let audioEl: HTMLAudioElement | undefined;

  let training = $state(false);
  let trainStage = $state<string>("queued");
  let trainProgress = $state(0);
  let trainLogs = $state<string[]>([]);
  let trainError = $state<string | undefined>(undefined);
  let trainSuccessPath = $state<string | undefined>(undefined);
  let unlistenTrain: (() => void | Promise<void>) | undefined;
  let logEl: HTMLDivElement | undefined = $state(undefined);

  $effect(() => {
    void logEl;
    void trainLogs.length;
    if (logEl) logEl.scrollTop = logEl.scrollHeight;
  });

  $effect(() => {
    void refreshDir();
    void refreshWords();
    return () => {
      void teardownRecorder();
      if (audioEl) {
        audioEl.pause();
        audioEl.removeAttribute("src");
      }
      void unlistenTrain?.();
    };
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
        selectWord(words[0].name);
      } else if (activeName) {
        const w = words.find((w) => w.name === activeName);
        if (w?.target_phrase) phraseInput = w.target_phrase;
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
    const w = words.find((w) => w.name === name);
    phraseInput = w?.target_phrase ?? "";
  }

  async function createWord() {
    const trimmed = nameInput.trim();
    if (!trimmed) return;
    if (!/^[A-Za-z0-9_-]+$/.test(trimmed)) {
      app.showToast("name must be ASCII letters/digits/_-", true);
      return;
    }
    activeName = trimmed;
    nameInput = "";
    phraseInput = "";
    if (!words.find((w) => w.name === trimmed)) {
      words = [...words, { name: trimmed, positive: 0, negative: 0, target_phrase: null }].sort(
        (a, b) => a.name.localeCompare(b.name),
      );
    }
  }

  let savePhraseTimer: ReturnType<typeof setTimeout> | undefined;
  function onPhraseInput() {
    if (!activeName) return;
    if (savePhraseTimer) clearTimeout(savePhraseTimer);
    savePhraseTimer = setTimeout(async () => {
      try {
        await dbus.saveWordMeta(activeName, phraseInput.trim());
        words = words.map((w) =>
          w.name === activeName ? { ...w, target_phrase: phraseInput.trim() || null } : w,
        );
      } catch (e) {
        app.showToast(`save phrase: ${formatErr(e)}`, true);
      }
    }, 400);
  }

  async function ensureRecorder(): Promise<Recorder> {
    if (recorder) return recorder;
    const r = await openRecorder();
    if (Math.abs(r.actualSampleRate - PCM_SAMPLE_RATE) > 1) {
      app.showToast(
        `mic at ${r.actualSampleRate} Hz (asked for ${PCM_SAMPLE_RATE}); samples will be off-rate`,
        true,
      );
    }
    unsubFrame = r.onFrame((f) => (micPeak = f.peak));
    recorder = r;
    return r;
  }

  async function teardownRecorder() {
    unsubFrame?.();
    unsubFrame = undefined;
    if (progressTimer) {
      clearInterval(progressTimer);
      progressTimer = undefined;
    }
    if (recorder) {
      const r = recorder;
      recorder = undefined;
      await r.close().catch(() => undefined);
    }
    micPeak = 0;
  }

  async function recordTake(kind: SampleKind) {
    if (!activeName) {
      recordError = "pick or create a word first";
      return;
    }
    if (recordingKind) return;
    recordError = undefined;
    recordingKind = kind;
    progress = 0;

    try {
      const r = await ensureRecorder();
      progressTimer = setInterval(() => {
        progress = Math.min(1, progress + 50 / TAKE_MS);
      }, 50);
      const samples16 = await r.recordFixed(TAKE_MS);
      const saved = await dbus.saveTrainingSample(
        activeName,
        kind,
        Math.round(r.actualSampleRate),
        samples16,
      );
      app.showToast(`saved ${kind} (${saved.duration_ms} ms)`);
      await Promise.all([refreshSamples(activeName), refreshWords()]);
    } catch (e) {
      recordError = formatErr(e);
    } finally {
      if (progressTimer) {
        clearInterval(progressTimer);
        progressTimer = undefined;
      }
      progress = 0;
      recordingKind = undefined;
    }
  }

  async function removeSample(path: string) {
    if (playingPath === path) stopPlayback();
    try {
      await dbus.deleteTrainingSample(path);
      await Promise.all([refreshSamples(activeName), refreshWords()]);
    } catch (e) {
      app.showToast(`delete failed: ${formatErr(e)}`, true);
    }
  }

  async function togglePlayback(path: string) {
    if (playingPath === path) {
      stopPlayback();
      return;
    }
    try {
      stopPlayback();
      const bytes = await dbus.readTrainingSample(path);
      const url = URL.createObjectURL(new Blob([bytes], { type: "audio/wav" }));
      const el = new Audio(url);
      audioEl = el;
      playingPath = path;
      el.addEventListener("ended", () => {
        if (playingPath === path) {
          playingPath = undefined;
          URL.revokeObjectURL(url);
          audioEl = undefined;
        }
      });
      await el.play();
    } catch (e) {
      app.showToast(`play failed: ${formatErr(e)}`, true);
    }
  }

  function stopPlayback() {
    if (audioEl) {
      audioEl.pause();
      audioEl.src && URL.revokeObjectURL(audioEl.src);
      audioEl = undefined;
    }
    playingPath = undefined;
  }

  function appendLog(line: string) {
    trainLogs = trainLogs.length >= MAX_LOG_LINES
      ? [...trainLogs.slice(1), line]
      : [...trainLogs, line];
  }

  async function tryTrain() {
    if (training) return;
    if (!activeName) return;
    if (!phraseInput.trim()) {
      app.showToast("set a target phrase first", true);
      return;
    }
    trainError = undefined;
    trainSuccessPath = undefined;
    trainLogs = [];
    trainStage = "starting";
    trainProgress = 0;
    training = true;

    try {
      unlistenTrain = await onTrain((evt) => {
        if (evt.kind === "log") {
          appendLog(evt.line);
        } else if (evt.kind === "status") {
          const p = evt.payload;
          if (typeof p.stage === "string") trainStage = p.stage;
          if (typeof p.progress === "number") trainProgress = p.progress;
          if (typeof p.error === "string") trainError = p.error;
          if (p.stage === "done" && typeof p.model === "string") {
            trainSuccessPath = p.model;
          }
        }
      });

      const onnxPath = await dbus.trainWakeword(activeName, phraseInput.trim());
      trainSuccessPath ??= onnxPath;
      trainProgress = 1;
      trainStage = "done";

      try {
        await dbus.add(activeName, onnxPath, 0.5, 1500);
        app.showToast(`registered ${activeName} with the daemon`);
      } catch (e) {
        app.showToast(`trained but registration failed: ${formatErr(e)}`, true);
      }
      await refreshWords();
    } catch (e) {
      trainError = formatErr(e);
      trainStage = "failed";
    } finally {
      training = false;
      void unlistenTrain?.();
      unlistenTrain = undefined;
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

  function fmtDuration(ms: number): string {
    if (!ms) return "—";
    return `${(ms / 1000).toFixed(2)}s`;
  }

  const positiveCount = $derived(samples.filter((s) => s.kind === "positive").length);
  const negativeCount = $derived(samples.filter((s) => s.kind === "negative").length);
  const phraseDisplay = $derived(phraseInput.trim() || "…");
</script>

<section class="pt-7">
  <header class="mb-6">
    <h2 class="label-tracked text-ink font-bold m-0">Train a wakeword</h2>
    <p class="text-muted text-[11px] mt-1.5 leading-[1.55]">
      Record short fixed-length takes of your target phrase; samples are
      stored as 16 kHz mono WAV under
      <code class="font-mono bg-paper-2 px-1.5 py-px text-[11px]">{trainingDir}</code>
      so the openWakeWord training subprocess can read them directly.
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
            void createWord();
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
          Target phrase for <code class="font-mono text-accent">{activeName}</code>
        </span>
        <span class="text-muted text-[10px]">spoken text the model should fire on</span>
      </header>
      <input
        class="w-full bg-paper-2 border border-rule px-3 py-2 font-mono text-[13px] text-ink
               focus:outline-2 focus:outline-accent focus:-outline-offset-1"
        type="text"
        placeholder="hey jarvis"
        bind:value={phraseInput}
        oninput={onPhraseInput}
      />
    </div>

    <div class="mb-6 pb-5 border-b border-rule-soft">
      <header class="flex items-baseline justify-between mb-2.5">
        <span class="label-tracked text-ink font-bold">
          Record 2 s takes
        </span>
        <span class="text-muted text-[10px]">
          {positiveCount} positive · {negativeCount} negative · 16 kHz mono PCM
        </span>
      </header>

      <div class="grid grid-cols-2 gap-3 mb-3">
        {#each ["positive", "negative"] as kind (kind)}
          {@const k = kind as SampleKind}
          {@const isMine = recordingKind === k}
          {@const otherActive = recordingKind !== undefined && !isMine}
          <button
            class="relative flex flex-col items-center justify-center gap-2 px-4 py-5 border-2 cursor-pointer transition-colors disabled:opacity-40 disabled:cursor-not-allowed overflow-hidden"
            class:border-ink={!isMine}
            class:border-accent={isMine}
            class:bg-paper-2={!isMine}
            class:bg-[color-mix(in_oklab,var(--color-accent)_15%,var(--color-paper))]={isMine}
            onclick={() => recordTake(k)}
            disabled={otherActive || (isMine && progress < 1)}
          >
            {#if isMine}
              <div
                class="absolute left-0 bottom-0 h-1 bg-accent transition-[width] duration-75"
                style:width={`${progress * 100}%`}
              ></div>
            {/if}
            <Mic
              size="22"
              strokeWidth={2.2}
              class={isMine ? "text-accent animate-pulse" : ""}
            />
            <span
              class="font-mono font-semibold text-[11px] tracking-[0.18em] uppercase"
              class:text-accent={isMine}
            >
              {isMine ? `Recording ${k}…` : `Record ${k}`}
            </span>
            <span class="font-mono text-[10px] text-muted">
              {#if isMine}
                say "{phraseDisplay}"
              {:else if k === "positive"}
                say the phrase
              {:else}
                speak / noise
              {/if}
            </span>
          </button>
        {/each}
      </div>

      {#if recorder}
        <div class="flex items-center gap-2 mb-2">
          <span class="label-tracked text-muted text-[9px] w-[58px]">MIC</span>
          <div class="flex-1 h-1.5 bg-paper-2 border border-rule-soft overflow-hidden">
            <div
              class="h-full transition-[width] duration-75"
              class:bg-ok={micPeak < 0.7}
              class:bg-accent={micPeak >= 0.7}
              style:width={`${Math.min(100, micPeak * 140)}%`}
            ></div>
          </div>
          <span class="font-mono text-[10px] text-muted w-[40px] text-right">
            {Math.round(micPeak * 100)}%
          </span>
        </div>
      {/if}

      {#if recordError}
        <div class="text-accent text-[12px] px-2.5 py-2 border border-accent
                    bg-[color-mix(in_oklab,var(--color-accent)_8%,var(--color-paper))] mb-3">
          {recordError}
        </div>
      {/if}

      <p class="m-0 text-[10px] leading-[1.6] text-muted">
        ~10 positives at varied tone/distance + a couple hundred
        negatives drawn from your room noise gives openWakeWord enough
        to augment from. Each take is exactly 2 s — pause briefly before
        speaking so the phrase sits at the end of the clip.
      </p>
    </div>

    <div class="mb-6">
      <header class="flex items-baseline justify-between mb-2.5">
        <span class="label-tracked text-ink font-bold">Takes</span>
        {#if samples.length > 0}
          <span class="text-muted text-[10px]">{samples.length} total</span>
        {/if}
      </header>

      {#if samples.length === 0}
        <div class="px-4 py-6 text-center text-muted border border-rule-soft font-display italic text-[14px]">
          No takes yet for <code class="font-mono not-italic text-[12px]">{activeName}</code>.
        </div>
      {:else}
        <div class="border border-rule-soft divide-y divide-[color:var(--color-rule-soft)]">
          {#each samples as s (s.path)}
            <div class="grid grid-cols-[auto_auto_1fr_auto_auto_auto] items-center gap-3 px-3 py-2">
              <span
                class="px-1.5 py-px font-mono font-semibold text-[9px] tracking-[0.18em] uppercase border"
                class:border-ok={s.kind === "positive"}
                class:text-ok={s.kind === "positive"}
                class:border-accent={s.kind === "negative"}
                class:text-accent={s.kind === "negative"}
              >
                {s.kind === "positive" ? "POS" : "NEG"}
              </span>
              <button
                class="inline-flex items-center justify-center w-7 h-7 border border-rule-soft bg-transparent text-ink cursor-pointer transition-colors hover:bg-ink hover:text-paper"
                onclick={() => togglePlayback(s.path)}
                aria-label={playingPath === s.path ? "Pause" : "Play"}
                title={playingPath === s.path ? "Pause" : "Play"}
              >
                {#if playingPath === s.path}
                  <Pause size="12" strokeWidth={2.4} />
                {:else}
                  <Play size="12" strokeWidth={2.4} />
                {/if}
              </button>
              <span class="font-mono text-[11px] text-ink whitespace-nowrap overflow-hidden text-ellipsis">
                {fmtTime(s.ts_ms)}
              </span>
              <span class="font-mono text-[10px] text-muted">{fmtDuration(s.duration_ms)}</span>
              <span class="font-mono text-[10px] text-muted">{s.sample_rate} Hz</span>
              <button
                class="inline-flex items-center justify-center w-7 h-7 border border-rule-soft bg-transparent text-muted cursor-pointer transition-colors hover:bg-accent hover:border-accent hover:text-paper"
                onclick={() => removeSample(s.path)}
                aria-label="Delete take"
                title="Delete take"
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
        disabled={training || positiveCount === 0 || !phraseInput.trim()}
      >
        {#if training}
          <Loader2 size="14" strokeWidth={2.2} class="animate-spin" />
          Training…
        {:else}
          <Hammer size="14" strokeWidth={2.2} />
          Train {activeName}
        {/if}
      </button>
      <p class="m-0 text-[10px] leading-[1.6] text-muted mt-2">
        Spawns <code class="font-mono bg-paper-2 px-1.5 py-px text-[10px]">python -m horchd_train</code>
        (positives + audiomentations augmentation + precomputed openWakeWord
        negatives → DNN classifier → ONNX). One-time setup:
        <code class="font-mono bg-paper-2 px-1.5 py-px text-[10px]">cd python && uv sync && uv run horchd-fetch-negatives</code>,
        then point <code class="font-mono bg-paper-2 px-1.5 py-px text-[10px]">$HORCHD_PYTHON</code> at the venv.
      </p>

      {#if training || trainLogs.length > 0 || trainError || trainSuccessPath}
        <div class="mt-4 border border-rule">
          <div class="flex items-center gap-2.5 px-3 py-2 border-b border-rule-soft bg-paper-2">
            {#if training}
              <span class="size-2 rounded-full bg-accent animate-pulse"></span>
            {:else if trainError}
              <AlertCircle size="13" class="text-accent" />
            {:else if trainSuccessPath}
              <Check size="13" class="text-ok" />
            {/if}
            <span class="font-mono text-[11px] text-ink">
              {trainStage}
            </span>
            <span class="ml-auto font-mono text-[10px] text-muted">
              {Math.round(trainProgress * 100)}%
            </span>
          </div>
          <div class="h-1 w-full bg-paper-2">
            <div
              class="h-full bg-accent transition-[width] duration-300"
              style:width={`${trainProgress * 100}%`}
            ></div>
          </div>
          <div
            bind:this={logEl}
            class="font-mono text-[10px] leading-[1.5] text-ink-soft bg-paper p-3 max-h-[260px] overflow-y-auto"
          >
            {#each trainLogs as line, i (i)}
              <div
                class:text-accent={line.startsWith("✗") || line.startsWith("⚠")}
                class:text-ok={line.startsWith("✓")}
              >{line || " "}</div>
            {/each}
            {#if trainLogs.length === 0}
              <div class="text-muted italic">waiting for output…</div>
            {/if}
          </div>
          {#if trainError}
            <div class="px-3 py-2 border-t border-rule-soft text-[11px] text-accent font-mono">
              {trainError}
            </div>
          {/if}
          {#if trainSuccessPath}
            <div class="px-3 py-2 border-t border-rule-soft text-[11px] font-mono text-muted">
              wrote <code class="text-ok">{trainSuccessPath}</code>
            </div>
          {/if}
        </div>
      {/if}
    </div>
  {/if}
</section>
