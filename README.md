# horchd.xyz

Source for the **horchd** landing page. Maps to Codeberg repo
[`NewtTheWolf/pages`](https://codeberg.org/NewtTheWolf/pages),
served at <https://horchd.xyz> via the `public/.domains` file.

The site is built with Astro (no framework, no Tailwind — just CSS) and
emits a single ~16 KB HTML page plus inlined critical styles.

## Develop

```sh
bun install
bun run dev          # http://localhost:4321
bun run build        # → dist/
bun run preview      # serve dist/ locally
```

## Deploy

Codeberg Pages serves the **default branch** of a repo named `pages`.
Source-branch / built-branch pattern:

| Branch   | What's there                  | Why                              |
| -------- | ----------------------------- | -------------------------------- |
| `source` | This Astro project (the src)  | Versioned source of truth        |
| `main`   | Contents of `dist/` only      | What Codeberg Pages actually serves |

After making changes on `source`:

```sh
bun run build
./deploy.sh          # see below
```

`deploy.sh` rebuilds, replaces `main` with `dist/` content, and pushes.

## Domain wiring

`public/.domains` ships `horchd.xyz` and `www.horchd.xyz`. DNS records
needed (via the registrar):

```
CNAME  horchd.xyz       NewtTheWolf.codeberg.page.
TXT    horchd.xyz       NewtTheWolf.codeberg.page
```

(Codeberg accepts either `CNAME` or `ALIAS`. For the apex use TXT + A/AAAA
fallback if your registrar doesn't support `ALIAS`. Codeberg's IPs as of
April 2025: `A 217.197.84.141`, `AAAA 2a0a:4580:103f:c0de::2`.)
