<script lang="ts">
  type Props = { running: boolean; reachable: boolean };
  let { running, reachable }: Props = $props();

  const tone = $derived(!reachable ? "stopped" : running ? "running" : "stopped");
  const label = $derived(!reachable ? "no daemon" : running ? "running" : "stopped");
</script>

<span
  class="inline-flex items-center gap-2 px-3 py-1.5 border bg-paper label-tracked font-semibold border-current"
  class:text-ok={tone === "running"}
  class:text-accent={tone === "stopped"}
>
  <span
    class="w-[7px] h-[7px] rounded-full"
    class:bg-ok={tone === "running"}
    class:animate-pulse-pill={tone === "running"}
    class:bg-muted={tone === "stopped"}
  ></span>
  <span>{label}</span>
</span>
