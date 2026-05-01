# docs.horchd.xyz

Source for the **horchd** documentation site, served at
<https://docs.horchd.xyz>. Lives as branches in the daemon repo
[`NewtTheWolf/horchd`](https://codeberg.org/NewtTheWolf/horchd):

| Branch in `NewtTheWolf/horchd` | What's there                       | Why                              |
| ------------------------------ | ---------------------------------- | -------------------------------- |
| `main`                         | Daemon Rust source                 | Untouched by this project        |
| `docs-src`                     | This Eleventy project (the source) | Versioned source of truth        |
| `docs`                   | Contents of `_site/` only          | What Codeberg Pages actually serves |

The site uses **Eleventy** + **Halfmoon** — the same stack and visual
language as Codeberg's own documentation, with the Codeberg-amber
accent swapped to the horchd phosphor amber and warm-near-black.

## Develop

```sh
bun install
bun run dev          # http://localhost:8080
bun run build        # → _site/
```

## Deploy

```sh
./deploy.sh          # builds + force-pushes _site/ to docs branch
```

## Domain wiring

`.domains` lists `docs.horchd.xyz`. DNS:

```
CNAME  docs.horchd.xyz  docs.horchd.NewtTheWolf.codeberg.page.
```

(no TXT needed when using CNAME for a subdomain.)

## Attribution

Site template adapted from
[`Codeberg/Documentation`](https://codeberg.org/Codeberg/Documentation)
under CC-BY-SA 4.0.
