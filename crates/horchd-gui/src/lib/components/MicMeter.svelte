<script lang="ts">
  type Props = { level: number; reachable: boolean };
  let { level, reachable }: Props = $props();

  // Map normalized peak |sample| to a perceptually fair bar.
  // Linear is useless here — typical speech peaks at 0.05..0.3 which
  // would barely fill 5..30% of the bar. dB scale (-60..0 dB → 0..100%)
  // matches what audio meters usually do.
  const NOISE_DB = -60;
  const dbPct = $derived.by(() => {
    const v = Math.max(0, Math.min(1, level));
    if (v <= 1e-5) return 0;
    const db = 20 * Math.log10(v); // -100 dB (very quiet) .. 0 dB (clipping)
    return Math.max(0, Math.min(100, ((db - NOISE_DB) / -NOISE_DB) * 100));
  });
  const tone = $derived(level > 0.5 ? "loud" : level > 0.005 ? "live" : "quiet");
  const dbReadout = $derived.by(() => {
    if (level <= 1e-5) return "—";
    const db = 20 * Math.log10(level);
    return `${db.toFixed(0)} dB`;
  });
</script>

<div class="mic" class:offline={!reachable} title="Microphone input (peak), {dbReadout}">
  <span class="label">MIC</span>
  <div class="track">
    <div
      class="fill"
      class:loud={tone === "loud"}
      class:live={tone === "live"}
      style:width="{dbPct}%"
    ></div>
  </div>
</div>

<style>
  .mic {
    display: inline-flex;
    align-items: center;
    gap: 8px;
    font-family: var(--font-mono);
  }
  .label {
    font-size: 9px;
    letter-spacing: 0.22em;
    color: var(--color-muted);
  }
  .track {
    position: relative;
    width: 80px;
    height: 6px;
    background: color-mix(in oklab, var(--color-rule) 20%, transparent);
    border: 1px solid var(--color-rule);
  }
  .fill {
    height: 100%;
    background: var(--color-muted);
    transition:
      width 0.08s linear,
      background 0.25s ease;
  }
  .fill.live {
    background: var(--color-ok);
  }
  .fill.loud {
    background: var(--color-accent);
  }
  .mic.offline .label,
  .mic.offline .fill {
    opacity: 0.35;
  }
</style>
