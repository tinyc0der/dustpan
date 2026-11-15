# Mole inventory

Seven read-only inventories of Mole, each claim with a `file:line` in Mole's source. `docs/PLAN.md` cites their row IDs.

| File | Covers | Mole source read |
|---|---|---|
| `mac-clean-purge-installer.md` | `clean`, `purge`, `installer`, the clean target catalog | `main` at `5dd93fe3` |
| `mac-uninstall-slim.md` | `uninstall`, `slim` | `main` at `5dd93fe3` |
| `mac-optimize-core-cli.md` | Dispatcher, every command and flag, `optimize`, `touchid`, `history`, `completion`, `update`, `remove`, UI conventions, config files | `main` at `5dd93fe3` |
| `mac-analyze-status-tui.md` | The Go `analyze` and `status` TUIs | `main` at `5dd93fe3` |
| `safety-invariants.md` | SAFE-01 to SAFE-39, never-delete lists, deletion pipeline, dry-run parity, privilege, logs, top incidents | `main` at `5dd93fe3`, with Windows notes |
| `windows.md` | The Windows CLI | `windows` at `59f3dc5a` |
| `linux.md` | The Linux CLI | `linux` at `4de579cc` |

The local Mole checkout moved from `ce56a5b3` to `5dd93fe3` by a fast-forward pull at 20:24 on 2026-11-15, before any inventory agent started. Every macOS file was read at `5dd93fe3`.

Agents read the code and ran nothing. A claim marked "inferred" was read from code and not executed. The Windows and Linux files fill their "macOS equivalent" columns from each branch's `SYNC.md`. Check those cells against the macOS files.

`scripts/inventory-gate.sh` checks that every Mole command and flag appears in these files. See `scripts/README.md`.
