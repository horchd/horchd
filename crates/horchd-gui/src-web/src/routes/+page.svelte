<script lang="ts">
  import { Plus, RotateCcw } from "@lucide/svelte";

  import AddModal from "$lib/components/AddModal.svelte";
  import Gauge from "$lib/components/Gauge.svelte";
  import StatusPill from "$lib/components/StatusPill.svelte";
  import Ticker from "$lib/components/Ticker.svelte";
  import Toast from "$lib/components/Toast.svelte";
  import WakeCard from "$lib/components/WakeCard.svelte";
  import { app } from "$lib/app.svelte";

  let addOpen = $state(false);

  $effect(() => {
    void app.start();
    return () => app.stop();
  });
</script>

<svelte:head>
  <title>horchd</title>
</svelte:head>

<main class="max-w-[880px] mx-auto px-8 pt-7 pb-16">
  <div class="stagger">
    <header class="grid grid-cols-[1fr_auto] items-end gap-5 hair-b pb-5">
      <div>
        <div class="wordmark text-[48px] leading-[0.9]">
          <span class="ink">horchd</span><span class="dot">.</span>
        </div>
        <div class="label-tracked text-(--color-muted) mt-1.5">
          Wakeword Detection · Session Bus
        </div>
      </div>
      <StatusPill running={app.status.running} reachable={app.reachable} />
    </header>

    <section class="readout grid gap-8 py-8 hair-b">
      <Gauge
        label="Audio capture"
        value={app.status.audio_fps}
        reachable={app.reachable}
        history={app.audioHistory}
      />
      <div class="divider"></div>
      <Gauge
        label="Inference"
        value={app.status.score_fps}
        reachable={app.reachable}
        history={app.scoreHistory}
      />
    </section>

    <div class="flex items-center justify-between pt-7 pb-3">
      <h2 class="label-tracked font-bold text-(--color-ink) text-[11px] m-0">Wakewords</h2>
      <div class="label-tracked text-(--color-muted)">{app.wakes.length} loaded</div>
    </div>

    <section class="wakes flex flex-col">
      {#if app.wakes.length === 0}
        <div class="empty hair">
          No wakewords. Use <code class="hair-soft">horchctl add</code> or the button below.
        </div>
      {:else}
        {#each app.wakes as wake (wake.name)}
          <WakeCard {wake} />
        {/each}
      {/if}
    </section>

    <div class="actions-bar flex gap-2 py-5 hair-b">
      <button class="btn primary hair" onclick={() => (addOpen = true)}>
        <Plus size="14" strokeWidth={2.5} /> Add wakeword
      </button>
      <button class="btn hair" onclick={() => app.reload()}>
        <RotateCcw size="14" strokeWidth={2.5} /> Reload config
      </button>
    </div>

    <Ticker />
  </div>
</main>

{#if addOpen}
  <AddModal onClose={() => (addOpen = false)} />
{/if}

<Toast />

<style>
  .ink {
    color: var(--color-ink);
  }
  .dot {
    color: var(--color-accent);
    font-style: normal;
  }

  .readout {
    grid-template-columns: 1fr 1px 1fr;
  }
  .divider {
    background: var(--color-rule-soft);
  }

  .empty {
    padding: 56px 32px;
    text-align: center;
    color: var(--color-muted);
    font-family: var(--font-display);
    font-size: 18px;
    font-style: italic;
  }
  .empty code {
    font-family: var(--font-mono);
    font-style: normal;
    font-size: 13px;
    padding: 2px 6px;
    background: var(--color-paper-2);
  }

  .btn {
    display: inline-flex;
    align-items: center;
    gap: 8px;
    font-family: var(--font-mono);
    font-weight: 600;
    font-size: 11px;
    letter-spacing: 0.18em;
    text-transform: uppercase;
    padding: 10px 18px;
    background: transparent;
    color: var(--color-ink);
    cursor: pointer;
    transition:
      background 0.18s ease,
      color 0.18s ease;
  }
  .btn:hover {
    background: var(--color-ink);
    color: var(--color-paper);
  }
  .btn.primary {
    background: var(--color-ink);
    color: var(--color-paper);
  }
  .btn.primary:hover {
    background: var(--color-accent);
    border-color: var(--color-accent);
  }
</style>
