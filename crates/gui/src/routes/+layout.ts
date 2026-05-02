// Tauri serves a single static page (SPA mode). SSR is meaningless and
// breaks calls to Tauri APIs from `load`, so we disable it. `prerender`
// is intentionally NOT set: per the official Tauri+SvelteKit guide, SPA
// mode uses adapter-static's fallback page rather than prerendering.
export const ssr = false;
