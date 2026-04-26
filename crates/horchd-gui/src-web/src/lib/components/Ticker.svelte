<script lang="ts">
  import { state } from "$lib/state.svelte";
  import { elapsed } from "$lib/utils";
</script>

<section class="ticker py-5 flex gap-3 items-center text-[11px] text-(--color-muted) min-h-[32px]">
  <span class="label-tracked font-semibold text-(--color-ink) flex-none">Recent fires</span>
  <div class="events flex gap-5 overflow-hidden flex-1 min-w-0">
    {#if state.recentFires.length === 0}
      <span class="opacity-50">— none yet —</span>
    {:else}
      {#each state.recentFires as fire (fire.ts_ms + "::" + fire.name)}
        {(state.tick, void 0)}
        <span class="event">
          <b>{fire.name}</b>
          <span class="score">{fire.score.toFixed(3)}</span>
          <span>· {elapsed(fire.ts_ms)}</span>
        </span>
      {/each}
    {/if}
  </div>
</section>

<style>
  .event {
    display: inline-flex;
    gap: 8px;
    align-items: baseline;
    font-variant-numeric: tabular-nums;
    white-space: nowrap;
    animation: enter 0.4s ease;
  }
  .event b {
    font-weight: 600;
    color: var(--color-ink);
  }
  .event .score {
    font-family: var(--font-display);
    font-weight: 600;
    color: var(--color-accent);
  }
</style>
