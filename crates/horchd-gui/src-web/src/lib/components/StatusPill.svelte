<script lang="ts">
  type Props = { running: boolean; reachable: boolean };
  let { running, reachable }: Props = $props();

  const tone = $derived(
    !reachable ? "stopped" : running ? "running" : "stopped",
  );
  const label = $derived(!reachable ? "no daemon" : running ? "running" : "stopped");
</script>

<span
  class="inline-flex items-center gap-2 px-3 py-1.5 hair label-tracked font-semibold bg-paper"
  class:running={tone === "running"}
  class:stopped={tone === "stopped"}
>
  <span class="dot"></span>
  <span>{label}</span>
</span>

<style>
  .dot {
    width: 7px;
    height: 7px;
    border-radius: 50%;
    background: var(--color-muted);
  }
  .running {
    color: var(--color-ok);
    border-color: currentColor;
  }
  .running .dot {
    background: var(--color-ok);
    box-shadow: 0 0 0 0 currentColor;
    animation: pulse 2.6s ease-in-out infinite;
  }
  .stopped {
    color: var(--color-accent);
    border-color: currentColor;
  }
</style>
