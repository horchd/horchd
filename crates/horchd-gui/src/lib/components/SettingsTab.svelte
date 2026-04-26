<script lang="ts">
  import { app } from "$lib/app.svelte";
  import { dbus } from "$lib/dbus";

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

<section class="settings">
  <header class="head">
    <h2 class="label-tracked">Settings</h2>
  </header>

  <div class="group">
    <div class="row">
      <div class="row-label">
        <span class="label-tracked">Daemon status</span>
        <span class="row-help">Live readout of the running horchd process</span>
      </div>
      <div class="row-value">
        <span class="badge" class:ok={app.reachable} class:bad={!app.reachable}>
          {app.reachable ? (app.status.running ? "running" : "stopped") : "no daemon"}
        </span>
        <span class="muted">·</span>
        <span class="muted">audio {app.status.audio_fps.toFixed(2)} fps · score {app.status.score_fps.toFixed(2)} fps</span>
      </div>
    </div>

    <div class="row">
      <div class="row-label">
        <span class="label-tracked">Models directory</span>
        <span class="row-help">Where <em>Import</em> + <em>horchctl import-pretrained</em> drop new <code>.onnx</code> files</span>
      </div>
      <div class="row-value">
        <code class="path">{modelsDir}</code>
        <button class="copy-btn" onclick={() => copy(modelsDir, "models dir")}>
          {copied === "models dir" ? "✓" : "copy"}
        </button>
      </div>
    </div>

    <div class="row">
      <div class="row-label">
        <span class="label-tracked">Reload config</span>
        <span class="row-help">Re-read <code>~/.config/horchd/config.toml</code> without dropping the audio thread</span>
      </div>
      <div class="row-value">
        <button class="action-btn" onclick={() => app.reload()}>Reload</button>
      </div>
    </div>
  </div>

  <div class="group">
    <header class="group-head">
      <span class="label-tracked group-label">Audio input device</span>
      <span class="muted help">drops the cpal stream and restarts the inference task</span>
    </header>
    <div class="device-row">
      <div class="select-wrap" class:disabled={switching}>
        <select class="select" bind:value={selectedDevice} disabled={switching}>
          {#each devices as dev (dev)}
            <option value={dev}>{dev === "default" ? "(host default)" : dev}</option>
          {/each}
        </select>
        <span class="chevron" aria-hidden="true">▾</span>
      </div>
      <label class="persist-toggle">
        <input type="checkbox" bind:checked={savePersist} />
        <span>persist to <code>config.toml</code></span>
      </label>
      <button class="action-btn" onclick={applyDevice} disabled={switching}>
        {switching ? "switching…" : "apply"}
      </button>
    </div>
    <p class="group-body small">
      cpal lists every PipeWire / PulseAudio / ALSA source the host knows
      about. <code>(host default)</code> follows whatever PipeWire / Pulse
      currently routes to.
    </p>
  </div>

  <div class="group">
    <header class="group-head">
      <span class="label-tracked group-label">About</span>
    </header>
    <p class="group-body">
      horchd · native multi-wakeword detection daemon ·
      <a href="https://github.com/horchd/horchd" target="_blank" rel="noopener">github.com/horchd/horchd</a>
    </p>
  </div>
</section>

<style>
  .settings {
    padding-top: 28px;
  }
  .head {
    margin-bottom: 24px;
  }
  .head h2 {
    margin: 0;
    color: var(--color-ink);
    font-size: 11px;
    font-weight: 700;
  }
  .group {
    margin-bottom: 32px;
    padding-bottom: 24px;
    border-bottom: 1px solid var(--color-rule-soft);
  }
  .group-head {
    display: flex;
    align-items: baseline;
    justify-content: space-between;
    margin-bottom: 10px;
  }
  .group-label {
    color: var(--color-ink);
    font-weight: 700;
  }
  .group-body {
    margin: 0;
    font-size: 12px;
    line-height: 1.6;
    color: var(--color-muted);
  }
  .group-body a {
    color: var(--color-accent);
    text-decoration: none;
    border-bottom: 1px solid currentColor;
  }
  .row {
    display: grid;
    grid-template-columns: 1fr auto;
    align-items: center;
    gap: 16px;
    padding: 14px 0;
    border-bottom: 1px solid var(--color-rule-soft);
  }
  .row:last-child {
    border-bottom: 0;
  }
  .row-label {
    display: flex;
    flex-direction: column;
    gap: 2px;
  }
  .row-help {
    font-size: 11px;
    color: var(--color-muted);
  }
  .row-value {
    display: inline-flex;
    align-items: center;
    gap: 8px;
    font-family: var(--font-mono);
  }
  .badge {
    padding: 2px 8px;
    border: 1px solid currentColor;
    font-size: 10px;
    letter-spacing: 0.18em;
    text-transform: uppercase;
    font-weight: 600;
  }
  .badge.ok { color: var(--color-ok); }
  .badge.bad { color: var(--color-accent); }
  .muted { color: var(--color-muted); font-size: 11px; }
  .path {
    font-size: 12px;
    color: var(--color-ink);
    background: var(--color-paper-2);
    padding: 4px 8px;
    border: 1px solid var(--color-rule-soft);
    max-width: 360px;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .copy-btn,
  .action-btn {
    font-family: var(--font-mono);
    font-weight: 600;
    font-size: 10px;
    letter-spacing: 0.2em;
    text-transform: uppercase;
    padding: 4px 10px;
    border: 1px solid var(--color-rule);
    background: transparent;
    color: var(--color-ink);
    cursor: pointer;
    transition: background 0.18s ease, color 0.18s ease;
  }
  .copy-btn:hover,
  .action-btn:hover {
    background: var(--color-ink);
    color: var(--color-paper);
  }
  .action-btn:disabled {
    opacity: 0.5;
    cursor: progress;
  }

  .device-row {
    display: grid;
    grid-template-columns: 1fr auto auto;
    gap: 12px;
    align-items: center;
    padding: 6px 0 12px;
  }
  .select-wrap {
    position: relative;
    min-width: 0;
  }
  .select {
    appearance: none;
    -webkit-appearance: none;
    -moz-appearance: none;
    width: 100%;
    font-family: var(--font-mono);
    font-size: 12px;
    font-weight: 500;
    padding: 8px 32px 8px 12px;
    background: var(--color-paper-2);
    border: 1px solid var(--color-rule);
    color: var(--color-ink);
    cursor: pointer;
    transition:
      background 0.18s ease,
      border-color 0.18s ease,
      color 0.18s ease;
    /* Long device names shouldn't break the row layout. */
    text-overflow: ellipsis;
    white-space: nowrap;
    overflow: hidden;
  }
  .select:hover {
    border-color: var(--color-ink);
    background: color-mix(in oklab, var(--color-paper-2) 70%, var(--color-paper-3));
  }
  .select:focus {
    outline: 2px solid var(--color-accent);
    outline-offset: -2px;
  }
  .select-wrap.disabled .select,
  .select:disabled {
    opacity: 0.5;
    cursor: progress;
  }
  .chevron {
    position: absolute;
    right: 10px;
    top: 50%;
    transform: translateY(-52%);
    pointer-events: none;
    font-size: 14px;
    line-height: 1;
    color: var(--color-muted);
    font-family: var(--font-mono);
  }
  .select-wrap:hover .chevron { color: var(--color-ink); }
  /* Browser dropdown popup (limited but worth doing). */
  .select option {
    background: var(--color-paper);
    color: var(--color-ink);
    font-family: var(--font-mono);
  }
  .persist-toggle {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    font-family: var(--font-mono);
    font-size: 11px;
    color: var(--color-muted);
    cursor: pointer;
  }
  .persist-toggle code {
    background: var(--color-paper-2);
    padding: 1px 5px;
    font-size: 10px;
  }
  .group-body.small {
    font-size: 11px;
    margin-top: 4px;
  }
  .help {
    font-size: 10px;
    text-transform: none;
    letter-spacing: 0;
  }
</style>
