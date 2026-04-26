<script lang="ts">
  type Props = { level: number; reachable: boolean };
  let { level, reachable }: Props = $props();

  const NOISE_DB = -60;
  const dbPct = $derived.by(() => {
    const v = Math.max(0, Math.min(1, level));
    if (v <= 1e-5) return 0;
    const db = 20 * Math.log10(v);
    return Math.max(0, Math.min(100, ((db - NOISE_DB) / -NOISE_DB) * 100));
  });
  const tone = $derived(level > 0.5 ? "loud" : level > 0.005 ? "live" : "quiet");
  const dbReadout = $derived.by(() => {
    if (level <= 1e-5) return "—";
    const db = 20 * Math.log10(level);
    return `${db.toFixed(0)} dB`;
  });
</script>

<div
  class="inline-flex items-center gap-2 font-mono"
  class:opacity-35={!reachable}
  title="Microphone input (peak), {dbReadout}"
>
  <span class="text-[9px] tracking-[0.22em] text-muted">MIC</span>
  <div class="relative w-20 h-1.5 border border-rule bg-[color-mix(in_oklab,var(--color-rule)_20%,transparent)]">
    <div
      class="h-full transition-[width,background] duration-100 ease-linear"
      class:bg-muted={tone === "quiet"}
      class:bg-ok={tone === "live"}
      class:bg-accent={tone === "loud"}
      style:width="{dbPct}%"
    ></div>
  </div>
</div>
