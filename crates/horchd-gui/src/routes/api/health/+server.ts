// Silences the periodic `[404] GET /api/health` spam in `cargo tauri dev`.
// Tauri-CLI pings this URL to verify the dev server is up.
export const prerender = false;

export function GET(): Response {
  return new Response("ok", { status: 200 });
}
