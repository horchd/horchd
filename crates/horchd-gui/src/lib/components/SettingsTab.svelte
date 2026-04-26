<script lang="ts">
  import { app } from "$lib/app.svelte";
  import { dbus } from "$lib/dbus";

  let modelsDir = $state<string>("…");
  let copied = $state<string | undefined>(undefined);

  $effect(() => {
    void (async () => {
      try {
        modelsDir = await dbus.modelsDir();
      } catch {
        modelsDir = "(unavailable)";
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
      <span class="soon">coming soon</span>
    </header>
    <p class="group-body">
      Switching the cpal input device live (without restarting the daemon)
      needs daemon-side <code>ListInputDevices</code> + <code>SetInputDevice</code>
      methods plus a graceful audio-thread restart. Tracked as Phase B of
      the device-picker work — until then, edit
      <code>[engine].device</code> in <code>config.toml</code> and run
      <code>horchctl reload</code>.
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
  .soon {
    font-size: 9px;
    letter-spacing: 0.22em;
    text-transform: uppercase;
    color: var(--color-accent);
    border: 1px dashed var(--color-accent);
    padding: 2px 8px;
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
</style>
