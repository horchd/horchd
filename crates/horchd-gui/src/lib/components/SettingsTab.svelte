<script lang="ts">
  import { Sparkles } from "@lucide/svelte";

  import { app } from "$lib/app.svelte";
  import { dbus } from "$lib/dbus";
  import { openLyna } from "$lib/lyna";

  let modelsDir = $state<string>("…");
  let copied = $state<string | undefined>(undefined);
  let devices = $state<string[]>([]);
  let selectedDevice = $state<string>("default");
  let savePersist = $state<boolean>(true);
  let switching = $state<boolean>(false);

  $effect(() => {
    void (async () => {
      try {
        modelsDir = await dbus.modelsDir();
      } catch {
        modelsDir = "(unavailable)";
      }
    })();
  });

  $effect(() => {
    void (async () => {
      try {
        const list = await dbus.listInputDevices();
        devices = ["default", ...list.filter((d) => d !== "default")];
      } catch {
        devices = ["default"];
      }
    })();
  });

  async function copy(text: string, label: string) {
    try {
      await navigator.clipboard.writeText(text);
      copied = label;
      setTimeout(() => (copied = undefined), 1400);
    } catch {
      app.showToast("clipboard unavailable", true);
    }
  }

  async function trainInLyna() {
    try {
      const where = await openLyna();
      app.showToast(
        where === "local"
          ? "opened Lyna at localhost:5173"
          : "Lyna isn't running locally — opened install instructions",
      );
    } catch (e) {
      app.showToast(`couldn't open Lyna: ${e instanceof Error ? e.message : String(e)}`, true);
    }
  }

  async function applyDevice() {
    if (!selectedDevice || switching) return;
    switching = true;
    try {
      await dbus.setInputDevice(selectedDevice, savePersist);
      app.showToast(
        `audio device → ${selectedDevice === "default" ? "host default" : selectedDevice}`,
      );
    } catch (e) {
      app.showToast(`set device failed: ${e instanceof Error ? e.message : String(e)}`, true);
    } finally {
      switching = false;
    }
  }
</script>

<section class="pt-7">
  <header class="mb-6">
    <h2 class="label-tracked text-ink font-bold m-0">Settings</h2>
  </header>

  <div class="mb-8 pb-6 border-b border-rule-soft">
    <div class="grid grid-cols-[1fr_auto] items-center gap-4 py-3.5 border-b border-rule-soft">
      <div class="flex flex-col gap-0.5">
        <span class="label-tracked">Daemon status</span>
        <span class="text-[11px] text-muted">Live readout of the running horchd process</span>
      </div>
      <div class="inline-flex items-center gap-2 font-mono">
        <span
          class="px-2 py-0.5 border border-current text-[10px] tracking-[0.18em] uppercase font-semibold"
          class:text-ok={app.reachable}
          class:text-accent={!app.reachable}
        >
          {app.reachable ? (app.status.running ? "running" : "stopped") : "no daemon"}
        </span>
        <span class="text-muted text-[11px]">·</span>
        <span class="text-muted text-[11px]">audio {app.status.audio_fps.toFixed(2)} fps · score {app.status.score_fps.toFixed(2)} fps</span>
      </div>
    </div>

    <div class="grid grid-cols-[1fr_auto] items-center gap-4 py-3.5 border-b border-rule-soft">
      <div class="flex flex-col gap-0.5">
        <span class="label-tracked">Models directory</span>
        <span class="text-[11px] text-muted">Where <em>Import</em> + <em>horchctl import-pretrained</em> drop new <code class="font-mono bg-paper-2 px-1.5 py-px text-[11px]">.onnx</code> files</span>
      </div>
      <div class="inline-flex items-center gap-2 font-mono">
        <code class="text-[12px] text-ink bg-paper-2 px-2 py-1 border border-rule-soft max-w-[360px] overflow-hidden text-ellipsis whitespace-nowrap">{modelsDir}</code>
        <button
          class="font-mono font-semibold text-[10px] tracking-[0.2em] uppercase px-2.5 py-1 border border-rule bg-transparent text-ink cursor-pointer transition-colors hover:bg-ink hover:text-paper"
          onclick={() => copy(modelsDir, "models dir")}
        >{copied === "models dir" ? "✓" : "copy"}</button>
      </div>
    </div>

    <div class="grid grid-cols-[1fr_auto] items-center gap-4 py-3.5">
      <div class="flex flex-col gap-0.5">
        <span class="label-tracked">Reload config</span>
        <span class="text-[11px] text-muted">Re-read <code class="font-mono bg-paper-2 px-1.5 py-px text-[11px]">~/.config/horchd/config.toml</code> without dropping the audio thread</span>
      </div>
      <div class="inline-flex items-center gap-2 font-mono">
        <button
          class="font-mono font-semibold text-[10px] tracking-[0.2em] uppercase px-2.5 py-1 border border-rule bg-transparent text-ink cursor-pointer transition-colors hover:bg-ink hover:text-paper"
          onclick={() => app.reload()}
        >Reload</button>
      </div>
    </div>
  </div>

  <div class="mb-8 pb-6 border-b border-rule-soft">
    <header class="flex items-baseline justify-between mb-2.5">
      <span class="label-tracked text-ink font-bold">Audio input device</span>
      <span class="text-muted text-[10px]">drops the cpal stream and restarts the inference task</span>
    </header>
    <div class="grid grid-cols-[1fr_auto_auto] items-center gap-3 py-1.5 pb-3">
      <div class="relative min-w-0" class:opacity-50={switching}>
        <select
          class="field-select w-full font-mono text-[12px] font-medium px-3 py-2 pr-8 bg-paper-2 border border-rule text-ink cursor-pointer transition-colors text-ellipsis whitespace-nowrap overflow-hidden hover:border-ink hover:bg-[color-mix(in_oklab,var(--color-paper-2)_70%,var(--color-paper-3))] focus:outline-2 focus:outline-accent focus:-outline-offset-2 disabled:opacity-50 disabled:cursor-progress"
          bind:value={selectedDevice}
          disabled={switching}
        >
          {#each devices as dev (dev)}
            <option value={dev} class="bg-paper text-ink font-mono">
              {dev === "default" ? "(host default)" : dev}
            </option>
          {/each}
        </select>
        <span class="absolute right-2.5 top-1/2 -translate-y-[52%] pointer-events-none text-[14px] leading-none text-muted font-mono">▾</span>
      </div>
      <label class="inline-flex items-center gap-1.5 font-mono text-[11px] text-muted cursor-pointer">
        <input type="checkbox" bind:checked={savePersist} />
        <span>persist to <code class="bg-paper-2 px-1.5 py-px text-[10px]">config.toml</code></span>
      </label>
      <button
        class="font-mono font-semibold text-[10px] tracking-[0.2em] uppercase px-2.5 py-1 border border-rule bg-transparent text-ink cursor-pointer transition-colors hover:bg-ink hover:text-paper disabled:opacity-50 disabled:cursor-progress"
        onclick={applyDevice}
        disabled={switching}
      >{switching ? "switching…" : "apply"}</button>
    </div>
    <p class="m-0 text-[11px] leading-[1.6] text-muted mt-1">
      cpal lists every PipeWire / PulseAudio / ALSA source the host knows
      about. <code class="font-mono bg-paper-2 px-1.5 py-px text-[11px]">(host default)</code>
      follows whatever PipeWire / Pulse currently routes to.
    </p>
  </div>

  <div class="mb-8 pb-6 border-b border-rule-soft">
    <header class="flex items-baseline justify-between mb-2.5">
      <span class="label-tracked text-ink font-bold">Train your own wakeword</span>
    </header>
    <p class="m-0 text-[12px] leading-[1.6] text-muted">
      horchd is intentionally trainerless — it loads any
      openWakeWord-compatible <code class="font-mono bg-paper-2 px-1.5 py-px text-[11px]">.onnx</code> classifier. Use
      <strong>Lyna</strong>, the companion trainer/studio, to record
      samples, pick TTS voices for synthetic data, run training, and
      export a model. Drop the resulting <code class="font-mono bg-paper-2 px-1.5 py-px text-[11px]">.onnx</code> back into
      <em>Add → Import</em>.
    </p>
    <div class="flex items-center gap-3.5 mt-3">
      <button
        class="inline-flex items-center gap-1.5 font-mono font-semibold text-[10px] tracking-[0.2em] uppercase px-2.5 py-1 border border-ink bg-ink text-paper cursor-pointer transition-colors hover:bg-accent hover:border-accent"
        onclick={trainInLyna}
      >
        <Sparkles size="13" /> Open Lyna
      </button>
      <span class="text-muted text-[10px]">probes <code class="font-mono bg-paper-2 px-1.5 py-px text-[10px]">localhost:5173</code> · falls back to GitHub</span>
    </div>
  </div>

  <div>
    <header class="flex items-baseline justify-between mb-2.5">
      <span class="label-tracked text-ink font-bold">About</span>
    </header>
    <p class="m-0 text-[12px] leading-[1.6] text-muted">
      horchd · native multi-wakeword detection daemon ·
      <a class="text-accent border-b border-current no-underline" href="https://github.com/horchd/horchd" target="_blank" rel="noopener">github.com/horchd/horchd</a>
    </p>
  </div>
</section>
