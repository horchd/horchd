<script lang="ts">
  type Props = { history: number[]; capacity?: number };
  let { history, capacity = 48 }: Props = $props();

  const max = $derived(Math.max(0.001, ...history, 14));
  const last = $derived(history.at(-1) ?? 0);

  const bars = $derived(
    Array.from({ length: capacity }, (_, i) => {
      const idx = i - (capacity - history.length);
      const v = idx >= 0 ? history[idx] : 0;
      const h = Math.max(1, Math.round((v / max) * 26));
      return { h, peak: v === last && v > 0 };
    }),
  );
</script>

<div class="spark">
  {#each bars as { h, peak } (h + "·" + peak)}
    <span class="bar" class:peak style:height={h + "px"}></span>
  {/each}
</div>

<style>
  .spark {
    margin-top: 14px;
    display: flex;
    align-items: flex-end;
    gap: 2px;
    height: 28px;
    border-bottom: 1px solid var(--color-rule-soft);
  }
  .bar {
    flex: 1;
    min-width: 2px;
    background: var(--color-ink-soft);
    transition:
      height 0.3s ease,
      background 0.3s ease;
  }
  .bar.peak {
    background: var(--color-accent);
  }
</style>
