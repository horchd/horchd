<script lang="ts">
  type Props = { level: number; reachable: boolean };
  let { level, reachable }: Props = $props();

  const pct = $derived(Math.max(0, Math.min(1, level)) * 100);
  const tone = $derived(level > 0.6 ? "loud" : level > 0.05 ? "live" : "quiet");
</script>

<div class="mic" class:offline={!reachable} title="Microphone input level">
  <span class="label">MIC</span>
  <div class="track">
    <div class="fill" class:loud={tone === "loud"} class:live={tone === "live"} style:width="{pct}%"></div>
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
