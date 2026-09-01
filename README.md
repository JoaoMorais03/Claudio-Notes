# Claudio Notes

A small macOS overlay notes app I built to learn **GPUI** — Zed's GPU-rendered UI framework — and the `gpui-component` library on top of it.

No Cloudflare, no backend, no agent orchestration. Just a floating window, a menu-bar extra, a global hotkey, and a local markdown vault. The whole point was to get my hands dirty with GPUI's view system, entity model, and reactive rendering without the Swift pain of AppKit.

## What it does

- Floating utility window, always on top while visible
- Menu bar extra: Show / Hide, New Note, Open Vault in Finder, Quit
- Global hotkey: **Control-Option-Space** (toggles from any app)
- One `.md` file per note, nested folders allowed (`dsa-python/two-pointers.md`)
- First launch seeds `community-notes/` into the vault — existing files are never overwritten
- Writes an `INDEX.md` at the vault root listing every note
- Hide with Esc or ⌘W (the process stays in the menu bar). New note: ⌘N. Quit only from the menu bar **Quit** item.

macOS only. Building on another OS fails with `Claudio Notes is macOS-only`.

## Why I built it

I wanted to understand GPUI from the inside: how entities, windows, and actions fit together, how `gpui-component` gives you sidebar, editor, and button primitives without writing raw GPU code, and where the framework still feels rough. It's a learning project, not a product — expect rough edges and breaking changes as GPUI evolves.

## Run

```
source "$HOME/.cargo/env" && cargo run
```

Store tests: `cargo test`. The app binary is macOS-only.

## Vault

```
~/Library/Application Support/Claudio Notes/
```

## Structure

- `src/store.rs` — pure Rust, no GPUI dependency. File I/O, seeding, path safety, `INDEX.md` generation. This is the part I'd keep in any rewrite.
- `src/app.rs` — the GPUI view: sidebar, markdown editor, search, debounced saves.
- `src/chrome.rs` — menu bar extra and global hotkey via `tray-icon` and `global-hotkey`.
- `community-notes/` — seeded cheatsheets (bash, git, docker, kubectl, AWS, DSA patterns, SOLID).

## What I'd do differently

- Pin GPUI to a specific rev instead of floating on `main` — the component API moves fast.
- Extract the store behind a trait so the same logic could back a Tauri or Dioxus shell later.
- The hotkey registration is best-effort; a real app would surface the Accessibility prompt more gracefully.

Delete from a note's context menu uses `remove_file` (not Trash).
