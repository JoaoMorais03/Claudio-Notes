# Claudio Notes

macOS overlay notes for Super.engineering. Super owns worktrees and agents; this app owns the durable vault — cheatsheets and markdown you keep next to the work.

One `.md` file per note, nested folders allowed (`dsa-python/two-pointers.md`). First launch seeds `community-notes/` into the vault (existing files are never overwritten).

macOS only. Building on another OS fails with `Claudio Notes is macOS-only`.

## Run

```
source "$HOME/.cargo/env" && cargo run
```

Store tests: `cargo test`. The app binary is macOS-only.

Hide with Esc or ⌘W (the process stays in the menu bar). New note: ⌘N. Quit only from the menu bar **Quit** item.

## Vault

```
~/Library/Application Support/Claudio Notes/
```

Point Super agents at `INDEX.md` in that folder. They can read the markdown; this app does not talk to `sc` or any Super IPC.

## Overlay

- Utility window, always on top while visible
- Menu bar extra: Show / Hide, New Note, Open Vault in Finder, Quit
- Global hotkey: **Control-Option-Space** (toggles from any app)

The hotkey needs Accessibility. If registration fails, enable **Claudio Notes** in **System Settings → Privacy & Security → Accessibility**, then relaunch.

Delete from a note’s context menu uses `remove_file` (not Trash).
