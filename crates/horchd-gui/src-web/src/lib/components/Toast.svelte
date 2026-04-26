<script lang="ts">
  import { state } from "$lib/state.svelte";
</script>

{#if state.toast}
  {@const t = state.toast}
  <div class="toast hair" class:error={t.isError} role="status">{t.msg}</div>
{/if}

<style>
  .toast {
    position: fixed;
    bottom: 20px;
    left: 50%;
    transform: translateX(-50%);
    padding: 10px 18px;
    background: var(--color-paper);
    font-size: 11px;
    letter-spacing: 0.16em;
    text-transform: uppercase;
    box-shadow: 4px 4px 0 var(--color-ink);
    z-index: 100;
    animation: slide-in 0.2s ease;
  }
  .toast.error {
    color: var(--color-accent);
    border-color: var(--color-accent);
    box-shadow: 4px 4px 0 var(--color-accent-dim);
  }
  @keyframes slide-in {
    from {
      opacity: 0;
      transform: translateX(-50%) translateY(8px);
    }
    to {
      opacity: 1;
      transform: translateX(-50%) translateY(0);
    }
  }
</style>
