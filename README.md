# Claudio Notes

Local notes for software engineers. Rust + GPUI. Optional sidecar for [Claudio](https://github.com/joaomorais03).

Not Rook. This folder used to be their landing page; the Mac app was never in this repo.

## v0

- Native window (always-on-top / global hotkey is the next brick)
- One `.md` file per note in `~/Library/Application Support/Claudio Notes/`
- Markdown editor with Tree-sitter
- First launch seeds `community-notes/`
- `⌘N` new · `⌘W` / Esc hide

Global hotkey over any app and the Claudio install button come next.

```
source "$HOME/.cargo/env"
cargo run
```
