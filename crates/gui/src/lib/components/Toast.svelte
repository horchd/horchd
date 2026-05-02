<script lang="ts">
  import { app } from "$lib/app.svelte";
</script>

{#if app.toast}
  {@const t = app.toast}
  {@const errorBoxShadow = "shadow-[4px_4px_0_var(--color-accent-dim)]"}
  {@const okBoxShadow = "shadow-[4px_4px_0_var(--color-ink)]"}
  <div
    class="fixed bottom-5 left-1/2 -translate-x-1/2 px-[18px] py-[10px] bg-paper border z-[100] animate-toast-in {t.isError ? `text-accent border-accent ${errorBoxShadow}` : `border-rule ${okBoxShadow}`}"
    role="status"
  >
    <span class="text-[11px] tracking-[0.16em] uppercase">{t.msg}</span>
  </div>
{/if}
