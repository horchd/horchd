<script lang="ts">
  import { app } from "$lib/app.svelte";

  type Props = { onClose: () => void };
  let { onClose }: Props = $props();

  let name = $state("");
  let model = $state("");
  let threshold = $state(0.5);
  let cooldown = $state(1500);
  let busy = $state(false);
  let error = $state<string | undefined>(undefined);

  async function submit(ev: SubmitEvent) {
    ev.preventDefault();
    error = undefined;
    if (!name.trim() || !model.trim()) {
      error = "name and model path are required";
      return;
    }
    busy = true;
    try {
      await app.add(name.trim(), model.trim(), threshold, cooldown);
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
</script>

<svelte:window onkeydown={onKey} />

<div class="scrim" role="dialog" aria-modal="true" aria-labelledby="add-modal-title">
  <button class="scrim-bg" onclick={onClose} aria-label="Close" tabindex="-1"></button>

  <form class="modal hair" onsubmit={submit}>
    <h3 id="add-modal-title" class="wordmark text-[28px] mb-5 leading-none">Add wakeword.</h3>

    <div class="mb-3">
      <label for="add-name" class="label-tracked text-(--color-muted) mb-1 block">Name</label>
      <input
        id="add-name"
        class="field-input"
        type="text"
        bind:value={name}
        autocomplete="off"
        placeholder="lyna"
      />
    </div>

    <div class="mb-3">
      <label for="add-model" class="label-tracked text-(--color-muted) mb-1 block">Model path (.onnx)</label>
      <input
        id="add-model"
        class="field-input"
        type="text"
        bind:value={model}
        spellcheck="false"
        placeholder="~/.local/share/horchd/models/lyna.onnx"
      />
    </div>

    <div class="grid grid-cols-2 gap-3 mb-4">
      <div>
        <label for="add-threshold" class="label-tracked text-(--color-muted) mb-1 block">Threshold</label>
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
        <label for="add-cooldown" class="label-tracked text-(--color-muted) mb-1 block">Cooldown (ms)</label>
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
      <div class="error mb-3">{error}</div>
    {/if}

    <div class="flex justify-end gap-2">
      <button type="button" class="btn hair" onclick={onClose}>Cancel</button>
      <button type="submit" class="btn primary hair" disabled={busy}>
        {busy ? "Registering…" : "Register"}
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
  .scrim-bg {
    position: absolute;
    inset: 0;
    background: color-mix(in oklab, var(--color-paper) 30%, transparent);
    backdrop-filter: blur(6px);
    -webkit-backdrop-filter: blur(6px);
    border: 0;
    cursor: default;
  }
  .modal {
    position: relative;
    z-index: 1;
    width: min(440px, calc(100vw - 32px));
    background: var(--color-paper);
    padding: 28px 28px 24px;
    box-shadow: 8px 8px 0 var(--color-ink);
  }
  .btn {
    font-family: var(--font-mono);
    font-weight: 600;
    font-size: 11px;
    letter-spacing: 0.18em;
    text-transform: uppercase;
    padding: 10px 18px;
    background: transparent;
    color: var(--color-ink);
    cursor: pointer;
    transition:
      background 0.18s ease,
      color 0.18s ease;
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
  .error {
    color: var(--color-accent);
    font-size: 12px;
    padding: 8px 10px;
    border: 1px solid var(--color-accent);
    background: color-mix(in oklab, var(--color-accent) 8%, var(--color-paper));
  }
</style>
