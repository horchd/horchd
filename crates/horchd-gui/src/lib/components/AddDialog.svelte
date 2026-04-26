<script lang="ts">
  import { Sparkles } from "@lucide/svelte";

  import { app } from "$lib/app.svelte";
  import { dbus } from "$lib/dbus";
  import { openLyna } from "$lib/lyna";

  type Props = { onClose: () => void };
  let { onClose }: Props = $props();

  type Mode = "import" | "register";
  let mode = $state<Mode>("import");

  let name = $state("");
  let model = $state("");
  let threshold = $state(0.5);
  let cooldown = $state(1500);
  let busy = $state(false);
  let error = $state<string | undefined>(undefined);
  let modelsDir = $state<string>("…");

  $effect(() => {
    void (async () => {
      try {
        modelsDir = await dbus.modelsDir();
      } catch {
        modelsDir = "(unavailable)";
      }
    })();
  });

  function pickFile() {
    const input = document.createElement("input");
    input.type = "file";
    input.accept = ".onnx";
    input.onchange = () => {
      const f = input.files?.[0];
      if (!f) return;
      // Browser can't always expose the path; Tauri's webview does.
      // @ts-expect-error Tauri injects `path` on File on Linux/macOS
      model = f.path ?? f.name;
      if (!name) {
        name = f.name.replace(/\.onnx$/, "").replace(/_v\d+\.\d+$/, "");
      }
    };
    input.click();
  }

  async function submit(ev: SubmitEvent) {
    ev.preventDefault();
    error = undefined;
    if (!name.trim() || !model.trim()) {
      error = "name and model path are required";
      return;
    }
    busy = true;
    try {
      if (mode === "import") {
        await app.importWakeword(name.trim(), model.trim(), threshold, cooldown);
      } else {
        await app.add(name.trim(), model.trim(), threshold, cooldown);
      }
      onClose();
    } catch (e) {
      error = e instanceof Error ? e.message : String(e);
    } finally {
      busy = false;
    }
  }

  function onKey(ev: KeyboardEvent) {
    if (ev.key === "Escape") onClose();
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
</script>

<svelte:window onkeydown={onKey} />

<div class="scrim" role="dialog" aria-modal="true" aria-labelledby="add-modal-title">
  <button class="scrim-bg" onclick={onClose} aria-label="Close" tabindex="-1"></button>

  <form class="modal" onsubmit={submit}>
    <header class="modal-head">
      <h3 id="add-modal-title" class="wordmark">Add wakeword.</h3>
      <p class="hint">
        <strong>Import</strong> copies the model into <code>{modelsDir}</code> first;
        <strong>Register</strong> uses the file in place. Need a custom
        wakeword? <button type="button" class="train-link" onclick={trainInLyna}>
          <Sparkles size="11" /> train one in Lyna
        </button>.
      </p>
    </header>

    <div class="modes label-tracked">
      <button
        type="button"
        class="mode"
        class:active={mode === "import"}
        onclick={() => (mode = "import")}
      >
        Import
      </button>
      <button
        type="button"
        class="mode"
        class:active={mode === "register"}
        onclick={() => (mode = "register")}
      >
        Register
      </button>
    </div>

    <div class="field">
      <label for="add-name" class="label-tracked">Name</label>
      <input
        id="add-name"
        class="field-input"
        type="text"
        bind:value={name}
        autocomplete="off"
        placeholder="lyna"
      />
    </div>

    <div class="field">
      <label for="add-model" class="label-tracked">Source <code>.onnx</code></label>
      <div class="file-row">
        <input
          id="add-model"
          class="field-input"
          type="text"
          bind:value={model}
          spellcheck="false"
          placeholder={mode === "import"
            ? "/path/to/your/model.onnx"
            : "~/.local/share/horchd/models/lyna.onnx"}
        />
        <button type="button" class="browse" onclick={pickFile}>Browse…</button>
      </div>
    </div>

    <div class="field grid-two">
      <div>
        <label for="add-threshold" class="label-tracked">Threshold</label>
        <input
          id="add-threshold"
          class="field-input"
          type="number"
          step="0.01"
          min="0"
          max="1"
          bind:value={threshold}
        />
      </div>
      <div>
        <label for="add-cooldown" class="label-tracked">Cooldown (ms)</label>
        <input
          id="add-cooldown"
          class="field-input"
          type="number"
          step="100"
          min="0"
          bind:value={cooldown}
        />
      </div>
    </div>

    {#if error}
      <div class="error">{error}</div>
    {/if}

    <div class="actions">
      <button type="button" class="btn ghost" onclick={onClose}>Cancel</button>
      <button type="submit" class="btn primary" disabled={busy}>
        {busy ? "Working…" : mode === "import" ? "Import + register" : "Register"}
      </button>
    </div>
  </form>
</div>

<style>
  .scrim {
    position: fixed;
    inset: 0;
    display: grid;
    place-items: center;
    z-index: 50;
    animation: fadein 0.2s ease;
  }
  @keyframes fadein {
    from { opacity: 0; }
    to { opacity: 1; }
  }
  .scrim-bg {
    position: absolute;
    inset: 0;
    background: color-mix(in oklab, var(--color-paper) 30%, transparent);
    backdrop-filter: blur(6px);
    border: 0;
    cursor: default;
  }
  .modal {
    position: relative;
    z-index: 1;
    width: min(520px, calc(100vw - 32px));
    background: var(--color-paper);
    border: 1px solid var(--color-rule);
    padding: 24px 24px 20px;
    box-shadow: 8px 8px 0 var(--color-ink);
  }
  .modal-head { margin-bottom: 16px; }
  .wordmark {
    margin: 0 0 6px;
    font-size: 26px;
    line-height: 1;
  }
  .hint {
    margin: 0;
    color: var(--color-muted);
    font-size: 12px;
    line-height: 1.5;
  }
  .hint code {
    font-family: var(--font-mono);
    font-size: 11px;
    background: var(--color-paper-2);
    padding: 1px 5px;
  }
  .train-link {
    display: inline-flex;
    align-items: center;
    gap: 4px;
    background: transparent;
    border: 0;
    padding: 0;
    font: inherit;
    color: var(--color-accent);
    border-bottom: 1px solid currentColor;
    cursor: pointer;
  }
  .train-link:hover {
    color: var(--color-ink);
  }

  .modes {
    display: inline-flex;
    margin-bottom: 18px;
    border: 1px solid var(--color-rule);
  }
  .mode {
    background: transparent;
    border: 0;
    border-right: 1px solid var(--color-rule-soft);
    padding: 6px 14px;
    font-weight: 600;
    color: var(--color-muted);
    cursor: pointer;
    transition: color 0.18s ease, background 0.18s ease;
  }
  .mode:last-child { border-right: 0; }
  .mode.active {
    background: var(--color-ink);
    color: var(--color-paper);
  }

  .field {
    margin-bottom: 14px;
  }
  .field label {
    display: block;
    color: var(--color-muted);
    margin-bottom: 4px;
  }
  .grid-two {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: 12px;
  }
  .file-row {
    display: grid;
    grid-template-columns: 1fr auto;
    gap: 8px;
  }
  .browse {
    font-family: var(--font-mono);
    font-weight: 600;
    font-size: 11px;
    letter-spacing: 0.16em;
    text-transform: uppercase;
    padding: 0 14px;
    border: 1px solid var(--color-rule);
    background: transparent;
    color: var(--color-ink);
    cursor: pointer;
    transition: background 0.18s ease, color 0.18s ease;
  }
  .browse:hover {
    background: var(--color-ink);
    color: var(--color-paper);
  }

  .error {
    color: var(--color-accent);
    font-size: 12px;
    padding: 8px 10px;
    border: 1px solid var(--color-accent);
    background: color-mix(in oklab, var(--color-accent) 8%, var(--color-paper));
    margin-bottom: 12px;
  }

  .actions {
    display: flex;
    justify-content: flex-end;
    gap: 8px;
    margin-top: 12px;
  }
  .btn {
    font-family: var(--font-mono);
    font-weight: 600;
    font-size: 11px;
    letter-spacing: 0.18em;
    text-transform: uppercase;
    padding: 10px 18px;
    border: 1px solid var(--color-rule);
    background: transparent;
    color: var(--color-ink);
    cursor: pointer;
    transition: background 0.18s ease, color 0.18s ease, border-color 0.18s ease;
  }
  .btn:hover:not(:disabled) {
    background: var(--color-ink);
    color: var(--color-paper);
  }
  .btn.primary {
    background: var(--color-ink);
    color: var(--color-paper);
  }
  .btn.primary:hover:not(:disabled) {
    background: var(--color-accent);
    border-color: var(--color-accent);
  }
  .btn:disabled {
    opacity: 0.5;
    cursor: progress;
  }
  .btn.ghost {
    color: var(--color-muted);
    border-color: var(--color-rule-soft);
  }
</style>
