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

  // Track which fire timestamp we already flashed for. Without this,
  // any fire on any wake triggers `app.lastFires = {...spread}` which
  // replaces the proxy ref, causing every CompactWake's effect to
  // re-run and replay its old flash.
  let lastFlashTs = -1;
  $effect(() => {
    const fire = app.lastFires[wake.name];
    if (!fire || !row) return;
    if (fire.ts_ms === lastFlashTs) return;
    lastFlashTs = fire.ts_ms;
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

  const trace = $derived(app.scoreTraces[wake.name] ?? []);
  const TRACE_W = 160;
  const TRACE_H = 22;
  const tracePath = $derived.by(() => {
    if (trace.length < 2) return "";
    const step = TRACE_W / Math.max(1, trace.length - 1);
    return trace
      .map((v, i) => {
        const y = TRACE_H - Math.max(0, Math.min(1, v)) * TRACE_H;
        return `${i === 0 ? "M" : "L"} ${(i * step).toFixed(2)} ${y.toFixed(2)}`;
      })
      .join(" ");
  });
  const traceFillPath = $derived.by(() => {
    if (trace.length < 2 || !tracePath) return "";
    return `${tracePath} L ${TRACE_W} ${TRACE_H} L 0 ${TRACE_H} Z`;
  });
  const thresholdY = $derived(TRACE_H - local * TRACE_H);

  // Fire counter in last 60 s — triggers tick re-render via app.tick.
  const recentFireCount = $derived(((_: number) => {
    const cutoff = Date.now() - 60_000;
    const arr = app.fireTimes[wake.name] ?? [];
    return arr.filter((t) => t >= cutoff).length;
  })(app.tick));

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
    <div class="name-row">
      <span class="name">{wake.name}</span>
      {#if recentFireCount > 0}
        <span class="fire-badge" title="Fires in the last 60 s">{recentFireCount}× / 60s</span>
      {/if}
    </div>
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
    <svg
      class="trace"
      class:over
      viewBox="0 0 {TRACE_W} {TRACE_H}"
      preserveAspectRatio="none"
      role="img"
      aria-label="Score trace, last ~30 s"
    >
      <line
        x1="0"
        x2={TRACE_W}
        y1={thresholdY}
        y2={thresholdY}
        class="trace-threshold"
        stroke-dasharray="3 3"
      />
      {#if traceFillPath}
        <path d={traceFillPath} class="trace-fill" />
      {/if}
      {#if tracePath}
        <path d={tracePath} class="trace-line" fill="none" />
      {/if}
    </svg>
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
  .name-row {
    display: flex;
    align-items: baseline;
    gap: 8px;
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
  .fire-badge {
    font-family: var(--font-mono);
    font-size: 9px;
    letter-spacing: 0.12em;
    text-transform: uppercase;
    font-weight: 600;
    color: var(--color-accent);
    border: 1px solid var(--color-accent);
    padding: 1px 6px;
    background: color-mix(in oklab, var(--color-accent) 8%, transparent);
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

  .trace {
    width: 100%;
    height: 22px;
    margin-top: 6px;
    display: block;
  }
  .trace-threshold {
    stroke: var(--color-rule-soft);
    stroke-width: 1;
    opacity: 0.8;
  }
  .trace-line {
    stroke: var(--color-ink-soft);
    stroke-width: 1.4;
    stroke-linejoin: round;
    stroke-linecap: round;
    transition: stroke 0.25s ease;
  }
  .trace-fill {
    fill: var(--color-ink-soft);
    opacity: 0.12;
    transition: fill 0.25s ease, opacity 0.25s ease;
  }
  .trace.over .trace-line { stroke: var(--color-accent); }
  .trace.over .trace-fill { fill: var(--color-accent); opacity: 0.18; }

  @keyframes wake-flash {
    0% {
      background: color-mix(in oklab, var(--color-accent) 38%, var(--color-paper));
      border-color: var(--color-accent);
      box-shadow: inset 4px 0 0 var(--color-accent);
    }
    30% {
      background: color-mix(in oklab, var(--color-accent) 22%, var(--color-paper));
      border-color: var(--color-accent);
      box-shadow: inset 4px 0 0 var(--color-accent);
    }
    100% {
      background: var(--color-paper);
      border-color: var(--color-rule);
      box-shadow: none;
    }
  }
  :global(.flash) {
    animation: wake-flash 1.6s cubic-bezier(0.2, 0.7, 0.2, 1);
  }
</style>
