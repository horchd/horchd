<script lang="ts">
  import { Save, X } from "@lucide/svelte";
  import { app } from "$lib/app.svelte";
  import { elapsed } from "$lib/utils";
  import type { WakewordRow } from "$lib/types";

  type Props = { wake: WakewordRow };
  let { wake }: Props = $props();

  let pending = $state<number | undefined>(undefined);
  const local = $derived(pending ?? wake.threshold);
  const dirty = $derived(pending !== undefined && pending !== wake.threshold);
  let row: HTMLElement | undefined = $state();
  let debounceId: ReturnType<typeof setTimeout> | undefined;

  $effect(() => {
    const fire = app.lastFires[wake.name];
    if (!fire || !row) return;
    const target = row;
    target.classList.remove("flash");
    void target.offsetWidth;
    target.classList.add("flash");
  });

  const lastFireLabel = $derived(((_: number) => {
    const f = app.lastFires[wake.name];
    return f ? elapsed(f.ts_ms) : "—";
  })(app.tick));

  const live = $derived(app.liveScores[wake.name]);
  const meterPct = $derived(Math.max(0, Math.min(1, live ?? 0)) * 100);
  const over = $derived(live !== undefined && live >= local);

  async function onInput(ev: Event) {
    const v = parseFloat((ev.target as HTMLInputElement).value);
    pending = v;
    if (debounceId) clearTimeout(debounceId);
    debounceId = setTimeout(() => app.setThreshold(wake.name, v, false), 220);
  }

  async function onSave() {
    await app.setThreshold(wake.name, local, true);
    pending = undefined;
  }

  async function toggle() {
    await app.setEnabledPersistent(wake.name, !wake.enabled);
  }

  async function onRemove() {
    if (!confirm(`Remove wakeword "${wake.name}"? The model file on disk is preserved.`)) return;
    await app.remove(wake.name);
  }
</script>

<article
  bind:this={row}
  class="wake"
  class:disabled={!wake.enabled}
  data-name={wake.name}
>
  <button
    class="toggle"
    class:on={wake.enabled}
    onclick={toggle}
    title={wake.enabled ? "Disable + persist" : "Enable + persist"}
    aria-label={wake.enabled ? "Disable" : "Enable"}
  >
    <span class="toggle-dot" class:fired={over && wake.enabled}></span>
  </button>

  <div class="name-col">
    <div class="name">{wake.name}</div>
    <div class="meta" title={wake.model}>
      {wake.cooldown_ms} ms · last fire {lastFireLabel}
    </div>
  </div>

  <div class="meter-col">
    <div class="meter-row">
      <div class="meter-wrap">
        <div class="meter-fill" class:over style:width="{meterPct}%"></div>
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
      <div class="readout">
        <span class="live" class:over>{live !== undefined ? live.toFixed(3) : "—"}</span>
        <span class="sep">/</span>
        <span class="threshold">{local.toFixed(3)}</span>
      </div>
    </div>
  </div>

  <div class="actions">
    <button
      class="icon-btn save-btn"
      class:dirty
      onclick={onSave}
      title="Persist threshold to config.toml"
      aria-label="Save threshold"
    >
      <Save size="13" />
    </button>
    <button
      class="icon-btn danger"
      onclick={onRemove}
      title="Remove wakeword (keeps the .onnx)"
      aria-label="Remove"
    >
      <X size="13" />
    </button>
  </div>
</article>

<style>
  .wake {
    display: grid;
    grid-template-columns: 36px minmax(140px, 1.2fr) minmax(220px, 3fr) auto;
    align-items: center;
    gap: 14px;
    padding: 12px 16px;
    border: 1px solid var(--color-rule);
    border-bottom: 0;
    background: var(--color-paper);
    transition:
      background 0.2s ease,
      border-color 0.4s ease;
  }
  .wake:last-child {
    border-bottom: 1px solid var(--color-rule);
  }
  .wake.disabled {
    opacity: 0.55;
  }

  .toggle {
    width: 28px;
    height: 28px;
    display: grid;
    place-items: center;
    border: 1px solid var(--color-rule);
    background: transparent;
    cursor: pointer;
    transition: background 0.18s ease, border-color 0.18s ease;
  }
  .toggle:hover {
    background: var(--color-paper-2);
  }
  .toggle-dot {
    width: 10px;
    height: 10px;
    border-radius: 50%;
    background: var(--color-rule-soft);
    transition: background 0.2s ease, box-shadow 0.2s ease;
  }
  .toggle.on .toggle-dot {
    background: var(--color-ok);
  }
  .toggle-dot.fired {
    background: var(--color-accent);
    box-shadow: 0 0 6px color-mix(in oklab, var(--color-accent) 60%, transparent);
  }

  .name-col {
    min-width: 0;
  }
  .name {
    font-family: var(--font-mono);
    font-weight: 600;
    font-size: 14px;
    color: var(--color-ink);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .meta {
    font-size: 10px;
    letter-spacing: 0.05em;
    color: var(--color-muted);
    margin-top: 2px;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }

  .meter-col {
    min-width: 0;
  }
  .meter-row {
    display: grid;
    grid-template-columns: 1fr auto;
    gap: 12px;
    align-items: center;
  }
  .meter-wrap {
    position: relative;
    height: 18px;
  }
  .meter-fill {
    position: absolute;
    left: 0;
    top: 50%;
    margin-top: -3px;
    height: 6px;
    background: var(--color-ink-soft);
    transition:
      width 0.18s ease,
      background 0.25s ease,
      opacity 0.25s ease;
    pointer-events: none;
    opacity: 0.42;
  }
  .meter-fill.over {
    background: var(--color-accent);
    opacity: 1;
  }
  .meter-wrap .slider {
    position: relative;
    z-index: 1;
  }

  .readout {
    font-family: var(--font-mono);
    font-size: 11px;
    color: var(--color-muted);
    font-variant-numeric: tabular-nums;
    display: inline-flex;
    align-items: baseline;
    gap: 4px;
    min-width: 7ch;
    text-align: right;
  }
  .readout .live.over {
    color: var(--color-accent);
    font-weight: 600;
  }
  .readout .sep {
    color: var(--color-rule-soft);
  }
  .readout .threshold {
    color: var(--color-ink);
  }

  .actions {
    display: inline-flex;
    gap: 4px;
  }
  .icon-btn {
    width: 26px;
    height: 26px;
    display: grid;
    place-items: center;
    border: 1px solid var(--color-rule-soft);
    background: transparent;
    color: var(--color-muted);
    cursor: pointer;
    transition: background 0.18s ease, color 0.18s ease, border-color 0.18s ease;
  }
  .icon-btn:hover {
    background: var(--color-ink);
    color: var(--color-paper);
    border-color: var(--color-ink);
  }
  .icon-btn.danger:hover {
    background: var(--color-accent);
    border-color: var(--color-accent);
    color: var(--color-paper);
  }
  .icon-btn.save-btn.dirty {
    color: var(--color-accent);
    border-color: var(--color-accent);
  }

  @keyframes wake-flash {
    0% {
      background: color-mix(in oklab, var(--color-accent) 22%, var(--color-paper));
      border-color: var(--color-accent);
    }
    100% {
      background: var(--color-paper);
      border-color: var(--color-rule);
    }
  }
  :global(.flash) {
    animation: wake-flash 0.9s cubic-bezier(0.2, 0.7, 0.2, 1);
  }
</style>
