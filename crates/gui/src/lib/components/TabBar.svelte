<script lang="ts">
  type Props = {
    tabs: Array<{ id: string; label: string; badge?: string | number }>;
    active: string;
    onSelect: (id: string) => void;
  };
  let { tabs, active, onSelect }: Props = $props();
</script>

<nav class="flex gap-0 mt-7 border-b border-rule" aria-label="Sections">
  {#each tabs as tab (tab.id)}
    <button
      type="button"
      class="px-[18px] py-3 inline-flex items-center gap-2 label-tracked font-semibold cursor-pointer
             border-r border-rule-soft last:border-r-0 transition-colors duration-150
             hover:text-ink"
      class:text-muted={tab.id !== active}
      class:text-ink={tab.id === active}
      class:bg-[color-mix(in_oklab,var(--color-paper-3)_50%,transparent)]={tab.id === active}
      class:shadow-[inset_0_-2px_0_var(--color-accent)]={tab.id === active}
      onclick={() => onSelect(tab.id)}
    >
      {tab.label}
      {#if tab.badge !== undefined}
        <span class="inline-block min-w-[18px] text-center px-[5px] py-px text-[9px] tracking-[0.1em] text-ink border border-rule-soft bg-[color-mix(in_oklab,var(--color-paper-3)_70%,transparent)]">
          {tab.badge}
        </span>
      {/if}
    </button>
  {/each}
</nav>
