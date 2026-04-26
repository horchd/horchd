<script lang="ts">
  import { Plus, RotateCcw } from "@lucide/svelte";

  import AddDialog from "$lib/components/AddDialog.svelte";
  import CompactWake from "$lib/components/CompactWake.svelte";
  import Gauge from "$lib/components/Gauge.svelte";
  import MicMeter from "$lib/components/MicMeter.svelte";
  import SettingsTab from "$lib/components/SettingsTab.svelte";
  import StatusPill from "$lib/components/StatusPill.svelte";
  import TabBar from "$lib/components/TabBar.svelte";
  import Ticker from "$lib/components/Ticker.svelte";
  import Toast from "$lib/components/Toast.svelte";
  import { app } from "$lib/app.svelte";

  let addOpen = $state(false);
  let activeTab = $state<"wakes" | "settings">("wakes");
  let filter = $state("");

  $effect(() => {
    void app.start();
    return () => app.stop();
  });

  const filteredWakes = $derived(
    filter.trim() === ""
      ? app.wakes
      : app.wakes.filter((w) =>
          w.name.toLowerCase().includes(filter.toLowerCase().trim()),
        ),
  );
  const enabledCount = $derived(app.wakes.filter((w) => w.enabled).length);
</script>

<svelte:head>
  <title>horchd</title>
</svelte:head>

<main class="max-w-[920px] mx-auto px-7 pt-6 pb-16">
  <div class="stagger">
    <header class="grid grid-cols-[1fr_auto] items-end gap-5 pb-[18px] border-b border-rule">
      <div class="min-w-0">
        <div class="wordmark text-[40px] leading-[0.9]">
          <span class="text-ink">horchd</span><span class="text-accent not-italic">.</span>
        </div>
        <div class="label-tracked text-muted mt-1">
          Wakeword Detection · Session Bus
        </div>
      </div>
      <div class="inline-flex items-center gap-3.5">
        <MicMeter level={app.status.mic_level} reachable={app.reachable} />
        <StatusPill running={app.status.running} reachable={app.reachable} />
      </div>
    </header>

    <section class="grid grid-cols-[1fr_1px_1fr] gap-8 py-6 pb-[22px] border-b border-rule items-stretch">
      <Gauge
        label="Audio capture"
        value={app.status.audio_fps}
        reachable={app.reachable}
        history={app.audioHistory}
      />
      <div class="bg-rule-soft"></div>
      <Gauge
        label="Inference"
        value={app.status.score_fps}
        reachable={app.reachable}
        history={app.scoreHistory}
      />
    </section>

    <TabBar
      tabs={[
        { id: "wakes", label: "Wakewords", badge: app.wakes.length },
        { id: "settings", label: "Settings" },
      ]}
      active={activeTab}
      onSelect={(id) => (activeTab = id as "wakes" | "settings")}
    />

    {#if activeTab === "wakes"}
      <section class="pt-[18px]">
        <div class="grid grid-cols-[1fr_auto_auto] gap-3 items-center mb-3.5">
          <input
            class="bg-paper-2 border border-rule-soft px-3 py-2 font-mono text-[12px] text-ink min-w-0
                   focus:outline-2 focus:outline-accent focus:-outline-offset-1"
            type="search"
            placeholder="Filter wakewords…"
            bind:value={filter}
            aria-label="Filter wakewords"
          />
          <span class="label-tracked text-muted text-[10px] whitespace-nowrap">
            {enabledCount} ENABLED · {app.wakes.length - enabledCount} OFF
          </span>
          <div class="inline-flex gap-2">
            <button
              class="inline-flex items-center gap-1.5 font-mono font-semibold text-[10px] tracking-[0.18em] uppercase
                     px-3.5 py-2 border border-ink bg-ink text-paper cursor-pointer transition-colors
                     hover:bg-accent hover:border-accent"
              onclick={() => (addOpen = true)}
            >
              <Plus size="13" strokeWidth={2.5} /> Add
            </button>
            <button
              class="inline-flex items-center gap-1.5 font-mono font-semibold text-[10px] tracking-[0.18em] uppercase
                     px-3.5 py-2 border border-rule-soft bg-transparent text-muted cursor-pointer
                     transition-colors hover:bg-ink hover:text-paper hover:border-ink"
              onclick={() => app.reload()}
              title="Re-read config.toml"
            >
              <RotateCcw size="13" strokeWidth={2.5} /> Reload
            </button>
          </div>
        </div>

        <div class="flex flex-col">
          {#if !app.reachable}
            <div class="px-6 py-12 text-center text-muted border border-rule font-display italic text-[17px]">
              horchd daemon is not on the session bus.
              <div class="mt-3.5 font-mono not-italic text-[12px] leading-[1.7] text-ink-soft">
                <code class="font-mono not-italic text-[13px] px-1.5 py-px bg-paper-2">systemctl --user start horchd</code><br />
                or run it manually with <code class="font-mono not-italic text-[13px] px-1.5 py-px bg-paper-2">./target/release/horchd</code>
              </div>
            </div>
          {:else if app.wakes.length === 0}
            <div class="px-6 py-12 text-center text-muted border border-rule font-display italic text-[17px]">
              No wakewords yet. Use <code class="font-mono not-italic text-[13px] px-1.5 py-px bg-paper-2">Add</code> to import one,
              or <code class="font-mono not-italic text-[13px] px-1.5 py-px bg-paper-2">horchctl import-pretrained --list</code> from the terminal.
            </div>
          {:else if filteredWakes.length === 0}
            <div class="px-6 py-12 text-center text-muted border border-rule font-display italic text-[17px]">
              Nothing matches <code class="font-mono not-italic text-[13px] px-1.5 py-px bg-paper-2">{filter}</code>.
            </div>
          {:else}
            {#each filteredWakes as wake (wake.name)}
              <CompactWake {wake} />
            {/each}
          {/if}
        </div>

        <Ticker />
      </section>
    {:else}
      <SettingsTab />
    {/if}
  </div>
</main>

{#if addOpen}
  <AddDialog onClose={() => (addOpen = false)} />
{/if}

<Toast />
