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

<div class="mt-3.5 flex items-end gap-[2px] h-7 border-b border-rule-soft">
  {#each bars as bar, i (i)}
    <span
      class="flex-1 min-w-[2px] transition-all duration-300"
      class:bg-ink-soft={!bar.peak}
      class:bg-accent={bar.peak}
      style:height={bar.h + "px"}
    ></span>
  {/each}
</div>
