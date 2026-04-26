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
    <header class="masthead">
      <div class="brand">
        <div class="wordmark text-[40px] leading-[0.9]">
          <span class="ink">horchd</span><span class="dot">.</span>
        </div>
        <div class="label-tracked text-(--color-muted) mt-1">
          Wakeword Detection · Session Bus
        </div>
      </div>
      <div class="meta">
        <MicMeter level={app.status.mic_level} reachable={app.reachable} />
        <StatusPill running={app.status.running} reachable={app.reachable} />
      </div>
    </header>

    <section class="readout">
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

    <TabBar
      tabs={[
        { id: "wakes", label: "Wakewords", badge: app.wakes.length },
        { id: "settings", label: "Settings" },
      ]}
      active={activeTab}
      onSelect={(id) => (activeTab = id as "wakes" | "settings")}
    />

    {#if activeTab === "wakes"}
      <section class="wakes-section">
        <div class="toolbar">
          <input
            class="search"
            type="search"
            placeholder="Filter wakewords…"
            bind:value={filter}
            aria-label="Filter wakewords"
          />
          <div class="counts">
            <span class="label-tracked text-(--color-muted)">
              {enabledCount} ENABLED · {app.wakes.length - enabledCount} OFF
            </span>
          </div>
          <div class="actions-bar">
            <button class="btn primary" onclick={() => (addOpen = true)}>
              <Plus size="13" strokeWidth={2.5} /> Add
            </button>
            <button class="btn ghost" onclick={() => app.reload()} title="Re-read config.toml">
              <RotateCcw size="13" strokeWidth={2.5} /> Reload
            </button>
          </div>
        </div>

        <div class="wakes">
          {#if !app.reachable}
            <div class="empty">
              horchd daemon is not on the session bus.
              <div class="empty-cmd">
                <code>systemctl --user start horchd</code><br />
                or run it manually with <code>./target/release/horchd</code>
              </div>
            </div>
          {:else if app.wakes.length === 0}
            <div class="empty">
              No wakewords yet. Use <code>Add</code> to import one,
              or <code>horchctl import-pretrained --list</code> from the terminal.
            </div>
          {:else if filteredWakes.length === 0}
            <div class="empty">
              Nothing matches <code>{filter}</code>.
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

<style>
  .ink {
    color: var(--color-ink);
  }
  .dot {
    color: var(--color-accent);
    font-style: normal;
  }

  .masthead {
    display: grid;
    grid-template-columns: 1fr auto;
    align-items: end;
    gap: 20px;
    padding-bottom: 18px;
    border-bottom: 1px solid var(--color-rule);
  }
  .brand {
    min-width: 0;
  }
  .meta {
    display: inline-flex;
    align-items: center;
    gap: 14px;
  }

  .readout {
    display: grid;
    grid-template-columns: 1fr 1px 1fr;
    gap: 32px;
    padding: 24px 0 22px;
    border-bottom: 1px solid var(--color-rule);
    align-items: stretch;
  }
  .divider {
    background: var(--color-rule-soft);
  }

  .wakes-section {
    padding-top: 18px;
  }
  .toolbar {
    display: grid;
    grid-template-columns: 1fr auto auto;
    gap: 12px;
    align-items: center;
    margin-bottom: 14px;
  }
  .search {
    background: var(--color-paper-2);
    border: 1px solid var(--color-rule-soft);
    padding: 8px 12px;
    font-family: var(--font-mono);
    font-size: 12px;
    color: var(--color-ink);
    min-width: 0;
  }
  .search:focus {
    outline: 2px solid var(--color-accent);
    outline-offset: -1px;
  }
  .counts {
    font-size: 10px;
    white-space: nowrap;
  }

  .actions-bar {
    display: inline-flex;
    gap: 8px;
  }
  .btn {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    font-family: var(--font-mono);
    font-weight: 600;
    font-size: 10px;
    letter-spacing: 0.18em;
    text-transform: uppercase;
    padding: 8px 14px;
    border: 1px solid var(--color-rule);
    background: transparent;
    color: var(--color-ink);
    cursor: pointer;
    transition: background 0.18s ease, color 0.18s ease, border-color 0.18s ease;
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
  .btn.ghost {
    color: var(--color-muted);
    border-color: var(--color-rule-soft);
  }

  .wakes {
    display: flex;
    flex-direction: column;
  }
  .empty {
    padding: 48px 24px;
    text-align: center;
    color: var(--color-muted);
    font-family: var(--font-display);
    font-size: 17px;
    font-style: italic;
    border: 1px solid var(--color-rule);
  }
  .empty code {
    font-family: var(--font-mono);
    font-style: normal;
    font-size: 13px;
    padding: 2px 6px;
    background: var(--color-paper-2);
  }
  .empty-cmd {
    margin-top: 14px;
    font-family: var(--font-mono);
    font-style: normal;
    font-size: 12px;
    line-height: 1.7;
    color: var(--color-ink-soft);
  }
</style>
