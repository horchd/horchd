<script lang="ts">
  import Spark from "./Spark.svelte";
  import { fmtFloat } from "$lib/utils";

  type Props = { label: string; value: number; reachable: boolean; history: number[] };
  let { label, value, reachable, history }: Props = $props();
</script>

<div>
  <div class="label-tracked text-muted mb-2">{label}</div>
  <div class="flex items-baseline">
    <span class="numeral text-[64px] leading-none" class:text-muted={!reachable}>
      {reachable ? fmtFloat(value) : "—"}
    </span>
    <span class="ml-2 label-tracked text-muted">fps</span>
  </div>
  <Spark {history} />
</div>
