export function elapsed(tsMs: number | undefined): string {
  if (!tsMs) return "never";
  const s = Math.max(0, Math.floor((Date.now() - tsMs) / 1000));
  if (s < 5) return "just now";
  if (s < 60) return `${s} s ago`;
  const m = Math.floor(s / 60);
  if (m < 60) return `${m} min ago`;
  const h = Math.floor(m / 60);
  return `${h} h ago`;
}

export function shortPath(p: string, n = 36): string {
  if (!p || p.length <= n) return p;
  return "…" + p.slice(p.length - n + 1);
}

export function fmtFloat(v: number | undefined, d = 2): string {
  return Number.isFinite(v) && v !== undefined ? v.toFixed(d) : "—";
}
