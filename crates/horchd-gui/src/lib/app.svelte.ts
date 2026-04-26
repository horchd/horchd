import { dbus, onDetected, onScore } from "./dbus";
import type { DaemonStatus, FireRecord, WakewordRow } from "./types";

const HISTORY_LEN = 48;

class HorchdState {
  status = $state<DaemonStatus>({ running: false, audio_fps: 0, score_fps: 0 });
  reachable = $state(true);
  wakes = $state<WakewordRow[]>([]);
  audioHistory = $state<number[]>([]);
  scoreHistory = $state<number[]>([]);
  /** name → most recent fire */
  lastFires = $state<Record<string, FireRecord>>({});
  /** name → live score (updated ~5 Hz from `horchd://score`) */
  liveScores = $state<Record<string, number>>({});
  /** newest fire first, capped */
  recentFires = $state<FireRecord[]>([]);
  /** UI tick counter so derived "x seconds ago" labels re-render */
  tick = $state(0);

  toast = $state<{ msg: string; isError: boolean; n: number } | null>(null);
  private toastN = 0;

  private statusTimer?: ReturnType<typeof setInterval>;
  private wakesTimer?: ReturnType<typeof setInterval>;
  private tickTimer?: ReturnType<typeof setInterval>;
  private unlistenDetected?: () => void | Promise<void>;
  private unlistenScore?: () => void | Promise<void>;

  showToast(msg: string, isError = false) {
    this.toastN += 1;
    this.toast = { msg, isError, n: this.toastN };
    const my = this.toastN;
    setTimeout(() => {
      if (this.toast?.n === my) this.toast = null;
    }, 2400);
  }

  async pollStatus() {
    try {
      const s = await dbus.status();
      this.status = s;
      this.reachable = true;
      this.audioHistory = pushCap(this.audioHistory, s.audio_fps);
      this.scoreHistory = pushCap(this.scoreHistory, s.score_fps);
    } catch {
      this.reachable = false;
    }
  }

  async refreshWakes() {
    try {
      this.wakes = await dbus.list();
    } catch (e) {
      this.showToast(`load failed: ${formatErr(e)}`, true);
    }
  }

  async toggle(name: string, enabled: boolean) {
    try {
      await dbus.setEnabled(name, enabled, false);
      await this.refreshWakes();
    } catch (e) {
      this.showToast(`toggle failed: ${formatErr(e)}`, true);
    }
  }

  async setThreshold(name: string, value: number, save: boolean) {
    try {
      await dbus.setThreshold(name, value, save);
      if (save) {
        this.showToast(`saved ${name} = ${value.toFixed(3)}`);
        await this.refreshWakes();
      }
    } catch (e) {
      this.showToast(`set threshold failed: ${formatErr(e)}`, true);
    }
  }

  async remove(name: string) {
    try {
      await dbus.remove(name);
      delete this.lastFires[name];
      this.showToast(`removed ${name}`);
      await this.refreshWakes();
    } catch (e) {
      this.showToast(`remove failed: ${formatErr(e)}`, true);
    }
  }

  async add(name: string, model: string, threshold: number, cooldownMs: number) {
    await dbus.add(name, model, threshold, cooldownMs);
    this.showToast(`added ${name}`);
    await this.refreshWakes();
  }

  async reload() {
    try {
      await dbus.reload();
      this.showToast("reloaded");
      await this.refreshWakes();
    } catch (e) {
      this.showToast(`reload failed: ${formatErr(e)}`, true);
    }
  }

  recordFire(name: string, score: number, tsMs: number) {
    const rec: FireRecord = { name, score, ts_ms: tsMs };
    this.lastFires = { ...this.lastFires, [name]: rec };
    this.recentFires = [rec, ...this.recentFires].slice(0, 8);
  }

  recordScore(name: string, score: number) {
    this.liveScores = { ...this.liveScores, [name]: score };
  }

  async start() {
    await this.pollStatus();
    await this.refreshWakes();
    this.statusTimer = setInterval(() => void this.pollStatus(), 1000);
    this.wakesTimer = setInterval(() => void this.refreshWakes(), 5000);
    this.tickTimer = setInterval(() => (this.tick += 1), 1000);
    this.unlistenDetected = await onDetected((p) =>
      this.recordFire(p.name, p.score, p.received_unix_ms ?? Date.now()),
    );
    this.unlistenScore = await onScore((p) => this.recordScore(p.name, p.score));
  }

  stop() {
    if (this.statusTimer) clearInterval(this.statusTimer);
    if (this.wakesTimer) clearInterval(this.wakesTimer);
    if (this.tickTimer) clearInterval(this.tickTimer);
    void this.unlistenDetected?.();
    void this.unlistenScore?.();
  }
}

function pushCap(arr: number[], v: number): number[] {
  const next = arr.length >= HISTORY_LEN ? arr.slice(1) : arr.slice();
  next.push(v);
  return next;
}

function formatErr(e: unknown): string {
  if (typeof e === "string") return e;
  if (e instanceof Error) return e.message;
  return String(e);
}

export const app = new HorchdState();
