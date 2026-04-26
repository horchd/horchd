<script lang="ts">
  type Props = {
    tabs: Array<{ id: string; label: string; badge?: string | number }>;
    active: string;
    onSelect: (id: string) => void;
  };
  let { tabs, active, onSelect }: Props = $props();
</script>

<nav class="tabs hair-b" aria-label="Sections">
  {#each tabs as tab (tab.id)}
    <button
      type="button"
      class="tab label-tracked"
      class:active={tab.id === active}
      onclick={() => onSelect(tab.id)}
    >
      {tab.label}
      {#if tab.badge !== undefined}
        <span class="badge">{tab.badge}</span>
      {/if}
    </button>
  {/each}
</nav>

<style>
  .tabs {
    display: flex;
    gap: 0;
    margin-top: 28px;
  }
  .tab {
    background: transparent;
    border: 0;
    border-right: 1px solid var(--color-rule-soft);
    padding: 12px 18px;
    font-weight: 600;
    color: var(--color-muted);
    cursor: pointer;
    transition: color 0.18s ease, background 0.18s ease;
    display: inline-flex;
    align-items: center;
    gap: 8px;
  }
  .tab:hover {
    color: var(--color-ink);
  }
  .tab.active {
    color: var(--color-ink);
    background: color-mix(in oklab, var(--color-paper-3) 50%, transparent);
    box-shadow: inset 0 -2px 0 var(--color-accent);
  }
  .badge {
    display: inline-block;
    min-width: 18px;
    text-align: center;
    padding: 1px 5px;
    background: color-mix(in oklab, var(--color-paper-3) 70%, transparent);
    border: 1px solid var(--color-rule-soft);
    font-size: 9px;
    letter-spacing: 0.1em;
    color: var(--color-ink);
  }
</style>
