---
eleventyNavigation:
  key: recipes
  title: Subscriber recipes
  icon: code
  order: 30
description: "Drop-in subscriber snippets for the xyz.horchd.Daemon1.Detected D-Bus signal in Bash, Python, and Rust."
---

Two flavours of recipe:

- **[Home Assistant](./home-assistant/)** — wire horchd into HA's voice
  pipeline as a Wyoming-protocol wake-word engine.
- **Subscriber snippets** — drop-in code for subscribing to
  `xyz.horchd.Daemon1.Detected` from the languages most likely to be on
  a horchd machine (Bash, Python, Rust).

{% sectionNav collections %}
