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

  let lastFlashTs = -1;
  $effect(() => {
    const fire = app.lastFires[wake.name];
    if (!fire || !row) return;
    if (fire.ts_ms === lastFlashTs) return;
    lastFlashTs = fire.ts_ms;
    const target = row;
    target.classList.remove("animate-flash");
    void target.offsetWidth;
    target.classList.add("animate-flash");
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
  class="grid items-center gap-3.5 px-4 py-3 bg-paper border border-rule
         border-b-0 last:border-b transition-colors duration-200
         grid-cols-[36px_minmax(140px,1.2fr)_minmax(220px,3fr)_auto]"
  class:opacity-55={!wake.enabled}
  data-name={wake.name}
>
  <button
    class="w-7 h-7 grid place-items-center border border-rule bg-transparent cursor-pointer
           transition-colors duration-150 hover:bg-paper-2"
    onclick={toggle}
    title={wake.enabled ? "Disable + persist" : "Enable + persist"}
    aria-label={wake.enabled ? "Disable" : "Enable"}
  >
    <span
      class="w-2.5 h-2.5 rounded-full transition-[background,box-shadow] duration-200"
      class:bg-rule-soft={!wake.enabled}
      class:bg-ok={wake.enabled && !(over && wake.enabled)}
      class:bg-accent={over && wake.enabled}
      class:shadow-[0_0_6px_color-mix(in_oklab,var(--color-accent)_60%,transparent)]={over && wake.enabled}
    ></span>
  </button>

  <div class="min-w-0">
    <div class="flex items-baseline gap-2">
      <span class="font-mono font-semibold text-[14px] text-ink overflow-hidden text-ellipsis whitespace-nowrap">{wake.name}</span>
      {#if recentFireCount > 0}
        <span
          class="font-mono text-[9px] tracking-[0.12em] uppercase font-semibold text-accent border border-accent px-1.5 py-px bg-[color-mix(in_oklab,var(--color-accent)_8%,transparent)]"
          title="Fires in the last 60 s"
        >{recentFireCount}× / 60s</span>
      {/if}
    </div>
    <div
      class="text-[10px] tracking-[0.05em] text-muted mt-0.5 whitespace-nowrap overflow-hidden text-ellipsis"
      title={wake.model}
    >
      {wake.cooldown_ms} ms · last fire {lastFireLabel}
    </div>
  </div>

  <div class="min-w-0">
    <div class="grid grid-cols-[1fr_auto] gap-3 items-center">
      <div class="relative h-[18px]">
        <div
          class="absolute left-0 top-1/2 -mt-[3px] h-1.5 pointer-events-none transition-[width,background,opacity] duration-200"
          class:bg-ink-soft={!over}
          class:opacity-40={!over}
          class:bg-accent={over}
          class:opacity-100={over}
          style:width="{meterPct}%"
        ></div>
        <input
          class="slider relative z-10"
          type="range"
          min="0"
          max="1"
          step="0.01"
          value={local}
          oninput={onInput}
        />
      </div>
      <div class="font-mono text-[11px] text-muted tabular-nums inline-flex items-baseline gap-1 min-w-[7ch] text-right">
        <span class:text-accent={over} class:font-semibold={over}>{live !== undefined ? live.toFixed(3) : "—"}</span>
        <span class="text-rule-soft">/</span>
        <span class="text-ink">{local.toFixed(3)}</span>
      </div>
    </div>
    <svg
      class="block w-full h-[22px] mt-1.5"
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
        class="stroke-rule-soft opacity-80"
        stroke-width="1"
        stroke-dasharray="3 3"
      />
      {#if traceFillPath}
        <path
          d={traceFillPath}
          class="transition-[fill,opacity] duration-200"
          class:fill-ink-soft={!over}
          class:opacity-[0.12]={!over}
          class:fill-accent={over}
          class:opacity-[0.18]={over}
        />
      {/if}
      {#if tracePath}
        <path
          d={tracePath}
          fill="none"
          stroke-width="1.4"
          stroke-linecap="round"
          stroke-linejoin="round"
          class="transition-[stroke] duration-200"
          class:stroke-ink-soft={!over}
          class:stroke-accent={over}
        />
      {/if}
    </svg>
  </div>

  <div class="inline-flex gap-1">
    <button
      class="w-[26px] h-[26px] grid place-items-center border bg-transparent cursor-pointer
             transition-colors duration-150 hover:bg-ink hover:text-paper hover:border-ink"
      class:text-muted={!dirty}
      class:border-rule-soft={!dirty}
      class:text-accent={dirty}
      class:border-accent={dirty}
      onclick={onSave}
      title="Persist threshold to config.toml"
      aria-label="Save threshold"
    >
      <Save size="13" />
    </button>
    <button
      class="w-[26px] h-[26px] grid place-items-center border border-rule-soft bg-transparent
             text-muted cursor-pointer transition-colors duration-150
             hover:bg-accent hover:text-paper hover:border-accent"
      onclick={onRemove}
      title="Remove wakeword (keeps the .onnx)"
      aria-label="Remove"
    >
      <X size="13" />
    </button>
  </div>
</article>
