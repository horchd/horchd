<script lang="ts">
  import { Check, AlertCircle, Loader2, Download, Boxes, ExternalLink } from "@lucide/svelte";

  import { app } from "$lib/app.svelte";
  import { dbus, onSetup } from "$lib/dbus";
  import type { TrainingEnvStatus } from "$lib/types";

  type Props = {
    onReady?: () => void;
  };
  let { onReady }: Props = $props();

  let status = $state<TrainingEnvStatus | undefined>(undefined);
  let busy = $state<"setup" | "fetch" | undefined>(undefined);
  let stage = $state<string>("idle");
  let progress = $state(0);
  let logs = $state<string[]>([]);
  let logEl: HTMLDivElement | undefined = $state(undefined);
  let unlisten: (() => void | Promise<void>) | undefined;

  $effect(() => {
    void refresh();
    return () => {
      void unlisten?.();
    };
  });

  $effect(() => {
    void logs.length;
    if (logEl) logEl.scrollTop = logEl.scrollHeight;
  });

  async function refresh() {
    try {
      status = await dbus.trainingEnvStatus();
      if (isReady(status)) onReady?.();
    } catch (e) {
      app.showToast(`env probe: ${formatErr(e)}`, true);
    }
  }

  function isReady(s: TrainingEnvStatus): boolean {
    return !!s.python_path && s.openwakeword_installed && s.negatives_present;
  }

  function appendLog(line: string) {
    logs = logs.length >= 200 ? [...logs.slice(1), line] : [...logs, line];
  }

  async function attachStream(): Promise<void> {
    void unlisten?.();
    unlisten = await onSetup((evt) => {
      if (evt.kind === "log") {
        appendLog(evt.line);
      } else if (evt.kind === "status") {
        const p = evt.payload;
        if (typeof p.stage === "string") stage = p.stage;
        if (typeof p.progress === "number") progress = p.progress;
      }
    });
  }

  async function setup() {
    if (busy) return;
    busy = "setup";
    logs = [];
    stage = "preparing";
    progress = 0;
    try {
      await attachStream();
      await dbus.setupTrainingEnv();
      app.showToast("training env ready");
    } catch (e) {
      app.showToast(`setup failed: ${formatErr(e)}`, true);
    } finally {
      busy = undefined;
      void unlisten?.();
      unlisten = undefined;
      await refresh();
    }
  }

  async function fetch() {
    if (busy) return;
    busy = "fetch";
    logs = [];
    stage = "downloading";
    progress = 0;
    try {
      await attachStream();
      await dbus.fetchNegatives();
      app.showToast("negatives feature file downloaded");
    } catch (e) {
      app.showToast(`fetch failed: ${formatErr(e)}`, true);
    } finally {
      busy = undefined;
      void unlisten?.();
      unlisten = undefined;
      await refresh();
    }
  }

  function formatErr(e: unknown): string {
    if (typeof e === "string") return e;
    if (e instanceof Error) return e.message;
    return String(e);
  }

  const ready = $derived(status ? isReady(status) : false);
  const uvOk = $derived(!!status?.uv_version);
  const venvOk = $derived(!!status?.python_path);
  const owwOk = $derived(!!status?.openwakeword_installed);
  const negOk = $derived(!!status?.negatives_present);
</script>

<div class="border border-rule mb-6">
  <header class="flex items-center gap-2.5 px-3.5 py-2.5 border-b border-rule-soft bg-paper-2">
    <Boxes size="14" strokeWidth={2.2} />
    <span class="label-tracked text-ink font-bold">Training environment</span>
    {#if ready}
      <span class="ml-auto inline-flex items-center gap-1 text-ok font-mono text-[10px] tracking-[0.18em] uppercase">
        <Check size="11" strokeWidth={2.6} /> ready
      </span>
    {:else}
      <span class="ml-auto font-mono text-[10px] text-muted">setup needed</span>
    {/if}
  </header>

  {#if status}
    <ul class="divide-y divide-[color:var(--color-rule-soft)]">
      <li class="grid grid-cols-[auto_1fr_auto] items-center gap-3 px-3.5 py-2">
        <span
          class="size-2"
          class:bg-ok={uvOk}
          class:bg-accent={!uvOk}
        ></span>
        <div class="min-w-0">
          <div class="font-mono text-[12px] text-ink">
            uv
            {#if uvOk}
              <span class="text-muted text-[10px] ml-1.5 selectable">{status.uv_version}</span>
            {/if}
          </div>
          <div class="text-[10px] text-muted">
            installs Python + the training package without polluting the system
          </div>
        </div>
        {#if !uvOk}
          <a
            class="inline-flex items-center gap-1 font-mono text-[10px] tracking-[0.18em] uppercase
                   px-2.5 py-1 border border-rule-soft text-ink hover:bg-ink hover:text-paper hover:border-ink transition-colors"
            href="https://docs.astral.sh/uv/getting-started/installation/"
            target="_blank"
            rel="noopener"
          >
            <ExternalLink size="11" strokeWidth={2.4} />
            install uv
          </a>
        {/if}
      </li>

      <li class="grid grid-cols-[auto_1fr_auto] items-center gap-3 px-3.5 py-2">
        <span
          class="size-2"
          class:bg-ok={venvOk && owwOk}
          class:bg-accent={uvOk && (!venvOk || !owwOk)}
          class:bg-rule-soft={!uvOk}
        ></span>
        <div class="min-w-0">
          <div class="font-mono text-[12px] text-ink">
            python venv + openwakeword
            {#if venvOk}
              <span class="text-muted text-[10px] ml-1.5 selectable">{status.python_path}</span>
            {/if}
          </div>
          <div class="text-[10px] text-muted">
            isolated env at <code class="font-mono bg-paper-2 px-1 selectable">{status.python_env_dir}</code>
          </div>
        </div>
        {#if uvOk}
          <button
            class="inline-flex items-center gap-1 font-mono font-semibold text-[10px] tracking-[0.18em] uppercase px-2.5 py-1 border border-ink bg-ink text-paper transition-colors hover:bg-accent hover:border-accent disabled:opacity-50 disabled:cursor-not-allowed"
            onclick={setup}
            disabled={busy !== undefined}
          >
            {#if busy === "setup"}
              <Loader2 size="11" strokeWidth={2.4} class="animate-spin" />
              installing…
            {:else if venvOk && owwOk}
              reinstall
            {:else}
              <Download size="11" strokeWidth={2.4} />
              install
            {/if}
          </button>
        {/if}
      </li>

      <li class="grid grid-cols-[auto_1fr_auto] items-center gap-3 px-3.5 py-2">
        <span
          class="size-2"
          class:bg-ok={negOk}
          class:bg-accent={venvOk && owwOk && !negOk}
          class:bg-rule-soft={!venvOk || !owwOk}
        ></span>
        <div class="min-w-0">
          <div class="font-mono text-[12px] text-ink">
            negatives feature file
            {#if negOk}
              <span class="text-muted text-[10px] ml-1.5">downloaded</span>
            {/if}
          </div>
          <div class="text-[10px] text-muted selectable">
            {status.negatives_features_path}
          </div>
        </div>
        {#if venvOk && owwOk}
          <button
            class="inline-flex items-center gap-1 font-mono font-semibold text-[10px] tracking-[0.18em] uppercase px-2.5 py-1 border border-ink bg-ink text-paper transition-colors hover:bg-accent hover:border-accent disabled:opacity-50 disabled:cursor-not-allowed"
            onclick={fetch}
            disabled={busy !== undefined}
          >
            {#if busy === "fetch"}
              <Loader2 size="11" strokeWidth={2.4} class="animate-spin" />
              downloading…
            {:else if negOk}
              re-fetch
            {:else}
              <Download size="11" strokeWidth={2.4} />
              fetch
            {/if}
          </button>
        {/if}
      </li>
    </ul>
  {:else}
    <div class="px-3.5 py-3 text-muted font-mono text-[11px] italic">probing…</div>
  {/if}

  {#if busy || logs.length > 0}
    <div class="border-t border-rule-soft">
      <div class="flex items-center gap-2 px-3.5 py-1.5 bg-paper-2">
        {#if busy}
          <Loader2 size="11" strokeWidth={2.4} class="animate-spin text-accent" />
        {/if}
        <span class="font-mono text-[10px] text-muted">{stage}</span>
        <span class="ml-auto font-mono text-[10px] text-muted">{Math.round(progress * 100)}%</span>
      </div>
      <div class="h-0.5 w-full bg-paper-2">
        <div class="h-full bg-accent transition-[width] duration-300" style:width={`${progress * 100}%`}></div>
      </div>
      <div
        bind:this={logEl}
        class="font-mono text-[10px] leading-[1.5] text-ink-soft bg-paper p-3 max-h-[180px] overflow-y-auto selectable"
      >
        {#each logs as line, i (i)}
          <div
            class:text-accent={line.startsWith("✗") || line.startsWith("⚠")}
            class:text-ok={line.startsWith("✓")}
          >{line || " "}</div>
        {/each}
      </div>
    </div>
  {/if}
</div>
