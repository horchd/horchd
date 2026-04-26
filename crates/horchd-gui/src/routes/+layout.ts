// Tauri serves the bundled frontend as static files; SSR is meaningless
// here, and prerendering means SvelteKit can compile to a self-contained
// `build/` directory for `frontendDist`.
export const prerender = true;
export const ssr = false;
