<script lang="ts">
  import { Sparkles } from "@lucide/svelte";

  import { app } from "$lib/app.svelte";
  import { dbus } from "$lib/dbus";
  import { openLyna } from "$lib/lyna";

  type Props = { onClose: () => void };
  let { onClose }: Props = $props();

  type Mode = "import" | "register";
  let mode = $state<Mode>("import");

  let name = $state("");
  let model = $state("");
  let threshold = $state(0.5);
  let cooldown = $state(1500);
  let busy = $state(false);
  let error = $state<string | undefined>(undefined);
  let modelsDir = $state<string>("…");

  $effect(() => {
    void (async () => {
      try {
        modelsDir = await dbus.modelsDir();
      } catch {
        modelsDir = "(unavailable)";
      }
    })();
  });

  function pickFile() {
    const input = document.createElement("input");
    input.type = "file";
    input.accept = ".onnx";
    input.onchange = () => {
      const f = input.files?.[0];
      if (!f) return;
      // @ts-expect-error Tauri injects `path` on File on Linux/macOS
      model = f.path ?? f.name;
      if (!name) {
        name = f.name.replace(/\.onnx$/, "").replace(/_v\d+\.\d+$/, "");
      }
    };
    input.click();
  }

  async function submit(ev: SubmitEvent) {
    ev.preventDefault();
    error = undefined;
    if (!name.trim() || !model.trim()) {
      error = "name and model path are required";
      return;
    }
    busy = true;
    try {
      if (mode === "import") {
        await app.importWakeword(name.trim(), model.trim(), threshold, cooldown);
      } else {
        await app.add(name.trim(), model.trim(), threshold, cooldown);
      }
      onClose();
    } catch (e) {
      error = e instanceof Error ? e.message : String(e);
    } finally {
      busy = false;
    }
  }

  function onKey(ev: KeyboardEvent) {
    if (ev.key === "Escape") onClose();
  }

  async function trainInLyna() {
    try {
      const where = await openLyna();
      app.showToast(
        where === "local"
          ? "opened Lyna at localhost:5173"
          : "Lyna isn't running locally — opened install instructions",
      );
    } catch (e) {
      app.showToast(`couldn't open Lyna: ${e instanceof Error ? e.message : String(e)}`, true);
    }
  }
</script>

<svelte:window onkeydown={onKey} />

<div class="fixed inset-0 grid place-items-center z-50 animate-fadein" role="dialog" aria-modal="true" aria-labelledby="add-modal-title">
  <button
    class="absolute inset-0 border-0 cursor-default
           bg-[color-mix(in_oklab,var(--color-paper)_30%,transparent)] backdrop-blur-md"
    onclick={onClose}
    aria-label="Close"
    tabindex="-1"
  ></button>

  <form
    class="relative z-10 w-[min(520px,calc(100vw-32px))] bg-paper border border-rule
           px-6 pt-6 pb-5 shadow-[8px_8px_0_var(--color-ink)]"
    onsubmit={submit}
  >
    <header class="mb-4">
      <h3 id="add-modal-title" class="wordmark mb-1.5 text-[26px] leading-none">Add wakeword.</h3>
      <p class="text-muted text-[12px] leading-[1.5]">
        <strong>Import</strong> copies the model into
        <code class="font-mono text-[11px] bg-paper-2 px-1.5 py-px">{modelsDir}</code> first;
        <strong>Register</strong> uses the file in place. Need a custom
        wakeword?
        <button
          type="button"
          class="inline-flex items-center gap-1 bg-transparent border-0 p-0 font-inherit
                 text-accent border-b border-current cursor-pointer hover:text-ink"
          onclick={trainInLyna}
        >
          <Sparkles size="11" /> train one in Lyna
        </button>.
      </p>
    </header>

    <div class="inline-flex mb-[18px] border border-rule label-tracked">
      {#each ["import", "register"] as m (m)}
        <button
          type="button"
          class="bg-transparent border-0 border-r border-rule-soft last:border-r-0 px-3.5 py-1.5
                 font-semibold cursor-pointer transition-colors duration-150"
          class:bg-ink={mode === m}
          class:text-paper={mode === m}
          class:text-muted={mode !== m}
          onclick={() => (mode = m as Mode)}
        >
          {m === "import" ? "Import" : "Register"}
        </button>
      {/each}
    </div>

    <div class="mb-3.5">
      <label for="add-name" class="block text-muted mb-1 label-tracked">Name</label>
      <input
        id="add-name"
        class="w-full px-2.5 py-2 bg-paper-2 border border-rule font-mono text-[13px] text-ink
               focus:outline-2 focus:outline-accent focus:-outline-offset-1"
        type="text"
        bind:value={name}
        autocomplete="off"
        placeholder="lyna"
      />
    </div>

    <div class="mb-3.5">
      <label for="add-model" class="block text-muted mb-1 label-tracked">
        Source <code class="font-mono">.onnx</code>
      </label>
      <div class="grid grid-cols-[1fr_auto] gap-2">
        <input
          id="add-model"
          class="w-full px-2.5 py-2 bg-paper-2 border border-rule font-mono text-[13px] text-ink
                 focus:outline-2 focus:outline-accent focus:-outline-offset-1"
          type="text"
          bind:value={model}
          spellcheck="false"
          placeholder={mode === "import"
            ? "/path/to/your/model.onnx"
            : "~/.local/share/horchd/models/lyna.onnx"}
        />
        <button
          type="button"
          class="font-mono font-semibold text-[11px] tracking-[0.16em] uppercase
                 px-3.5 border border-rule bg-transparent text-ink cursor-pointer
                 transition-colors duration-150 hover:bg-ink hover:text-paper"
          onclick={pickFile}
        >Browse…</button>
      </div>
    </div>

    <div class="grid grid-cols-2 gap-3 mb-4">
      <div>
        <label for="add-threshold" class="block text-muted mb-1 label-tracked">Threshold</label>
        <input
          id="add-threshold"
          class="w-full px-2.5 py-2 bg-paper-2 border border-rule font-mono text-[13px] text-ink
                 focus:outline-2 focus:outline-accent focus:-outline-offset-1"
          type="number"
          step="0.01"
          min="0"
          max="1"
          bind:value={threshold}
        />
      </div>
      <div>
        <label for="add-cooldown" class="block text-muted mb-1 label-tracked">Cooldown (ms)</label>
        <input
          id="add-cooldown"
          class="w-full px-2.5 py-2 bg-paper-2 border border-rule font-mono text-[13px] text-ink
                 focus:outline-2 focus:outline-accent focus:-outline-offset-1"
          type="number"
          step="100"
          min="0"
          bind:value={cooldown}
        />
      </div>
    </div>

    {#if error}
      <div class="text-accent text-[12px] px-2.5 py-2 border border-accent
                  bg-[color-mix(in_oklab,var(--color-accent)_8%,var(--color-paper))] mb-3">
        {error}
      </div>
    {/if}

    <div class="flex justify-end gap-2 mt-3">
      <button
        type="button"
        class="font-mono font-semibold text-[11px] tracking-[0.18em] uppercase px-[18px] py-2.5
               border border-rule-soft bg-transparent text-muted cursor-pointer
               transition-colors duration-150 hover:bg-ink hover:text-paper"
        onclick={onClose}
      >Cancel</button>
      <button
        type="submit"
        class="font-mono font-semibold text-[11px] tracking-[0.18em] uppercase px-[18px] py-2.5
               border border-rule bg-ink text-paper cursor-pointer transition-colors duration-150
               disabled:opacity-50 disabled:cursor-progress
               hover:not-disabled:bg-accent hover:not-disabled:border-accent"
        disabled={busy}
      >
        {busy ? "Working…" : mode === "import" ? "Import + register" : "Register"}
      </button>
    </div>
  </form>
</div>
