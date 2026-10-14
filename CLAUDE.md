# NeoTime repository: notes for Claude

This repository holds both halves of NeoTime, licensed GPL-3.0-or-later:

- [neotime/](neotime/) is the watch: firmware, bootloader, apps, faces, simulator, tools and docs.
  Read [neotime/CLAUDE.md](neotime/CLAUDE.md) before working there, and run its commands from inside it.
- [companion/](companion/) is the phone and desktop companion app. Read
  [companion/CLAUDE.md](companion/CLAUDE.md) before working there, and run its commands from inside it.

The phone-to-watch protocol is [neotime/docs/companion-protocol.md](neotime/docs/companion-protocol.md).
A change to it updates the firmware, the doc and the companion together, in one commit where it can.

The two halves are separate Cargo workspaces, so the companion's desktop crates never change how the
watch builds. `nix develop` gives the watch's toolchain and `nix develop .#companion` the
companion's; `nix develop .#companion-android` adds the Android SDK. Each half has its own CI
workflow under `.github/workflows`, run only when its files or the flake change.

## Working alongside other sessions

- Several Claude sessions work in this repository at once, each in its own worktree under `.claude/worktrees`.
- Talk to the other sessions whenever it helps: find them with `ListAgents` and message them with
  `SendMessage`. Agree on an order when one feature depends on another's landing, say which files
  you are about to change when they might be editing them too, and ask before touching a part
  another session owns.
- Work lands by pushing to GitHub's `master`, so the local `master` is often behind. Run
  `git fetch origin`, rebase onto `origin/master`, rerun the tests, then push without force.
- Never name a commit to anyone until it is on `origin/master`.
- Never revert or reformat another session's files. Run `cargo fmt -p <crate>`, not a bare
  `cargo fmt` over a whole workspace.
- `Cargo.lock` is the usual merge conflict; regenerate it rather than hand-merging.
- Before using the real watch, tell the other sessions, so only one connects at a time.
