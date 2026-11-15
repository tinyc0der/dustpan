# Decision trail for the dustpan plan

| When | Decision | Why | Evidence |
|---|---|---|---|
| 2026-11-15 | Ignore `mole-rs/` and `bruma-cleaner/` prior designs | Operator instruction | Operator reply |
| 2026-11-15 | Scope is every Mole feature on macOS, then Windows, then Linux, TUI plus CLI | Operator instruction | Operator reply |
| 2026-11-15 | Build the inventory from the dispatchers and command scripts, not the READMEs | READMEs can lag the code | `scripts/inventory-gate.sh` |
| 2026-11-15 | Seven read-only inventory agents, split by area and platform | Mole's three branches hold 118,450 lines of Bash, PowerShell and non-test Go, counted with `wc -l` (macOS 59,055 Bash and 14,239 Go; Windows 10,684 PowerShell and 10,172 Go; Linux 9,463 Bash and 14,837 Go) | `docs/inventory/*.md` |
| 2026-11-15 | Relabel the macOS inventories from `ce56a5b3` to `5dd93fe3` | The Mole checkout fast-forwarded at 20:24, before the agents read it | `git reflog` in the Mole checkout |
| 2026-11-15 | 19 gate tokens ignored as flags of tools Mole calls | Each was checked at its call site | `scripts/README.md` |
| 2026-11-15 | Gate proven with seeded controls (`--with-data`, `--gpu-shaders`, `--proc-cpu-window` removed from a copy) | A clean gate result means nothing without a known-bad run | Control run exited 1 and named each token |
| 2026-11-15 | Working name "dustpan"; Mole's trademark rules out "Mole" and `mo` | `TRADEMARK.md` | Operator to confirm the final name |
| 2026-11-15 | Default license GPL-3.0-or-later | Catalogs and rules are ported from GPL-3.0 code | Operator may override |
| 2026-11-15 | Read-only features (analyze, status) and the safety core land before any destructive command | Each milestone proves the base the next one deletes with | `docs/PLAN.md` milestones |
| 2026-11-15 | Caches and build artifacts are deleted for good, while installers, analyze deletions and uninstalled apps go to the Trash | Trash frees nothing for rebuildable data, and user-made files deserve recovery | `mac-clean-purge-installer.md` porting note 1 |
| 2026-11-15 | Add `dustpan explain PATH` | It gives the guard a user-facing surface in M0 without a test-only delete command (Mole incident I-64) | `docs/M0-checklist.md` Appendix B |
| 2026-11-15 | PR-level checklist only for M0 | Windows and Linux file lists do not exist yet | `docs/M0-checklist.md` |
| 2026-11-15 | No prototypes before the plan | Operator asked for the plan first; five spikes open M0 | `docs/PLAN.md` Appendix A |
| 2026-11-15 | Review fixes: recheck runs all five layers (SAFE-11); SAFE rules placed by body; Confirm moved to M1; `history` moved to M0; Linux F14, F18, F20 cells corrected from `linux.md` section 2; GPL worded as a conservative default; measured line counts replace estimates | Independent review found each contradiction or unsourced figure | `docs/PLAN.md`, gate run against PLAN.md alone |
| 2026-11-15 | Gate made self-contained: extracts the Windows and Linux branches itself and prints the three SHAs | A result must name what it checked | `# sources: main 5dd93fe3, windows 59f3dc5a, linux 4de579cc` |
| 2026-11-15 | 12 M0 lanes tagged Operator-run or CI-run | Touch ID, password, UAC, sudo and Windows desktop lanes have no unattended driver | `docs/M0-checklist.md` Appendix C |
| 2026-11-15 | Name "dustpan" confirmed | The operator created the repository as `dustpan` | `github.com/tinyc0der/dustpan` |
| 2026-11-15 | Default license GPL-3.0-or-later kept | The operator did not override it; `LICENSE` is the FSF text | `LICENSE`, `Cargo.toml` |
| 2026-11-15 | Hosting is `tinyc0der/dustpan`, public, trunk `main` | Operator instruction | `Cargo.toml` `repository` |
| 2026-11-15 | The Intel macOS runner is `macos-15-intel`, not `macos-13` | GitHub retired `macos-13` in late 2025; `macos-15-intel` is the Intel image it offers | `.github/workflows/ci.yml` |
| 2026-11-15 | Root refusal exits 3 (Refused), not 1 | `docs/PLAN.md` and the M0 checklist set 3; SAFE-16's test line says 1 and predates the exit table | `docs/PLAN.md` exit table |
| 2026-11-15 | `--version` reads the OS through `sysctlbyname`, `/etc/os-release` and `RtlGetVersion`, with no subprocess | Keeps it under the 20 ms budget and outside SAFE-19 and SAFE-20 | `crates/dustpan-sys/src/os.rs` |
| 2026-11-15 | The Apple Silicon macOS runner is `macos-15`, not `macos-14` | CI on this repo failed `macos-14` with "configuration macos-14-arm64-us-default is not supported" | check-run 132839324106 annotation |
