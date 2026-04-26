<script lang="ts">
  import { Save, X } from "@lucide/svelte";
  import { state } from "$lib/state.svelte";
  import { elapsed, shortPath } from "$lib/utils";
  import type { WakewordRow } from "$lib/types";

  type Props = { wake: WakewordRow };
  let { wake }: Props = $props();

  let local = $state(wake.threshold);
  let dirty = $state(false);
  let card: HTMLElement | undefined = $state();
  let debounceId: ReturnType<typeof setTimeout> | undefined;

  // Keep local slider in sync with the snapshot from the daemon when we
  // haven't dirtied it ourselves.
  $effect(() => {
    if (!dirty) local = wake.threshold;
  });

  // Flash the card when this wake fires.
  $effect(() => {
    const fire = state.lastFires[wake.name];
    if (!fire || !card) return;
    const target = card;
    target.classList.remove("flash");
    void target.offsetWidth;
    target.classList.add("flash");
  });

  // "x seconds ago" label re-renders when state.tick advances.
  const lastFireLabel = $derived(((_: number) => {
    const f = state.lastFires[wake.name];
    return f ? elapsed(f.ts_ms) : "never";
  })(state.tick));

  async function onInput(ev: Event) {
    const v = parseFloat((ev.target as HTMLInputElement).value);
    local = v;
    dirty = true;
    if (debounceId) clearTimeout(debounceId);
    debounceId = setTimeout(() => state.setThreshold(wake.name, v, false), 220);
  }

  async function onSave() {
    await state.setThreshold(wake.name, local, true);
    dirty = false;
  }

  async function onRemove() {
    if (!confirm(`Remove wakeword "${wake.name}"? The model file on disk is preserved.`)) return;
    await state.remove(wake.name);
  }
</script>

<article
  bind:this={card}
  class="wake hair p-5 bg-paper grid items-start"
  class:disabled={!wake.enabled}
  data-name={wake.name}
>
  <div class="marker"></div>

  <div class="header">
    <div class="font-semibold text-[17px] tracking-[0.01em]">{wake.name}</div>
    <div class="flex items-center gap-2">
      <button
        class="toggle hair label-tracked font-semibold px-2.5 py-1 cursor-pointer transition"
        class:on={wake.enabled}
        class:off={!wake.enabled}
        onclick={() => state.toggle(wake.name, !wake.enabled)}
        title={wake.enabled ? "Disable" : "Enable"}
      >
        {wake.enabled ? "ON" : "OFF"}
      </button>
      <button
        class="icon-btn hair w-[26px] h-[26px] grid place-items-center cursor-pointer transition"
        onclick={onRemove}
        title="Remove"
        aria-label="Remove {wake.name}"
      >
        <X size="14" />
      </button>
    </div>
  </div>

  <div class="body">
    <div class="flex-1">
      <div class="flex justify-between items-baseline label-tracked text-(--color-muted) mb-1">
        <span>Threshold</span>
        <span class="val">{local.toFixed(3)}</span>
      </div>
      <input
        class="slider"
        type="range"
        min="0"
        max="1"
        step="0.01"
        value={local}
        oninput={onInput}
      />
    </div>
    <button
      class="save-btn label-tracked px-1.5 py-0.5 cursor-pointer transition"
      class:dirty
      onclick={onSave}
      title="Persist threshold to config.toml"
    >
      <Save size="12" /> Save
    </button>
  </div>

  <div class="stats text-[11px] text-(--color-muted) mt-3 flex gap-5 flex-wrap">
    <span>Cooldown · <b>{wake.cooldown_ms} ms</b></span>
    <span>Last fire · <b>{lastFireLabel}</b></span>
    <span class="model" title={wake.model}>{shortPath(wake.model, 50)}</span>
  </div>
</article>

<style>
  .wake {
    grid-template-columns: auto 1fr;
    grid-template-areas:
      "marker header"
      "marker body"
      "marker stats";
    column-gap: 16px;
    row-gap: 4px;
    border-bottom-width: 0;
    transition:
      background 0.2s ease,
      border-color 0.4s ease;
  }
  .wake + :global(.wake) {
    margin-top: 0;
  }
  /* full bottom border on the last card in the list */
  :global(.wakes > .wake:last-child) {
    border-bottom-width: 1px;
  }
  .wake.disabled .header > div:first-child,
  .wake.disabled .body,
  .wake.disabled .stats {
    opacity: 0.46;
  }

  .marker {
    grid-area: marker;
    width: 5px;
    align-self: stretch;
    background: var(--color-rule);
    transition:
      background 0.4s ease,
      opacity 0.2s ease;
  }
  .wake.disabled .marker {
    opacity: 0.35;
  }

  .header {
    grid-area: header;
    display: flex;
    justify-content: space-between;
    align-items: center;
  }

  .body {
    grid-area: body;
    display: flex;
    align-items: end;
    gap: 12px;
    margin-top: 8px;
  }

  .val {
    font-family: var(--font-display);
    font-weight: 600;
    font-variation-settings: "opsz" 14;
    letter-spacing: -0.01em;
    font-size: 16px;
    color: var(--color-ink);
  }

  .toggle:hover {
    background: var(--color-ink);
    color: var(--color-paper);
  }
  .toggle.on {
    color: var(--color-ok);
    border-color: currentColor;
  }
  .toggle.off {
    color: var(--color-muted);
  }

  .icon-btn {
    color: var(--color-muted);
  }
  .icon-btn:hover {
    background: var(--color-accent);
    color: var(--color-paper);
    border-color: var(--color-accent);
  }

  .save-btn {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    color: var(--color-muted);
    border: 1px dashed var(--color-rule-soft);
  }
  .save-btn:hover {
    color: var(--color-ink);
    border-color: var(--color-ink);
  }
  .save-btn.dirty {
    color: var(--color-accent);
    border-color: var(--color-accent);
    border-style: solid;
  }

  .stats {
    grid-area: stats;
  }
  .stats b {
    font-weight: 500;
    color: var(--color-ink-soft);
    font-variant-numeric: tabular-nums;
  }
  .model {
    direction: rtl;
    text-align: left;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    max-width: 100%;
  }
</style>
