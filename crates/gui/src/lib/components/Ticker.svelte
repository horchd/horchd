<script lang="ts">
  import { app } from "$lib/app.svelte";
  import { elapsed } from "$lib/utils";
</script>

<section class="py-5 flex gap-3 items-center text-[11px] text-muted min-h-8" aria-live="polite">
  <span class="label-tracked font-semibold text-ink flex-none">Recent fires</span>
  <div class="flex gap-5 overflow-hidden flex-1 min-w-0">
    {#if app.recentFires.length === 0}
      <span class="opacity-50">— none yet —</span>
    {:else}
      {#each app.recentFires as fire (fire.ts_ms + "::" + fire.name)}
        {(app.tick, void 0)}
        <span class="inline-flex gap-2 items-baseline tabular-nums whitespace-nowrap animate-enter">
          <b class="font-semibold text-ink">{fire.name}</b>
          <span class="font-display font-semibold text-accent">{fire.score.toFixed(3)}</span>
          <span>· {elapsed(fire.ts_ms)}</span>
        </span>
      {/each}
    {/if}
  </div>
</section>
