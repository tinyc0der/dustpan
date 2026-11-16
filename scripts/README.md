# scripts

`inventory-gate.sh` lists every subcommand and long flag in Mole's three dispatchers, command scripts, and Go TUI flag declarations. It prints each one that `docs/inventory/` never names and exits 1 if any is missing. Its first line names the three Mole SHAs it checked.

Run it from the repository root:

```sh
./scripts/inventory-gate.sh
```

It reads macOS from the Mole checkout at `MAC` (default `../mole`, a Mole checkout beside this repository). It extracts `origin/windows` and `origin/linux` from that checkout into temp folders unless `WIN` and `LNX` point at extracted trees. `INV` overrides the folder it checks, which defaults to `docs/inventory`.

`inventory-gate.ignore` lists flags that belong to tools Mole calls, not to Mole:

- macOS `--print0`, `--show-errors` are `fd` flags (`bin/clean.sh:150`). `--required` is an argument of the internal `_run_cleanup_step` (`bin/clean.sh:1787`).
- Windows `--get`, `--git-path`, `--porcelain`, `--short`, `--show-current`, `--show-toplevel`, `--untracked-files` are `git` flags in `bin/update.ps1`. `--uninstall` is part of an uninstall-string pattern match (`bin/uninstall.ps1:737`).
- Linux `--format` (`docker system df`), `--no-legend`, `--no-pager` (`coredumpctl`), `--print-format` (`pacman`), `--runtime` (`flatpak`), `--quiet` (`systemctl`), `--short` (`git`), `--wait` (an `$EDITOR` comment).

To prove the gate still catches gaps, copy the inventory, rename a real flag in the copy, and point the gate at the copy. The run must print `MISSING` for that flag and exit 1.

```sh
cp -R docs/inventory /tmp/inv-control
sed -i '' 's/--with-data/--REMOVED/g' /tmp/inv-control/*.md
INV=/tmp/inv-control ./scripts/inventory-gate.sh   # expect MISSING macos --with-data and exit 1
```

## Bundle-id list generator

`scripts/gen-bundle-globs.sh` writes `crates/dustpan-core/src/protect/macos_bundles.rs` from Mole's `SYSTEM_CRITICAL_BUNDLES` and `DATA_PROTECTED_BUNDLES`. It reads `lib/core/app_protection_data.sh` at the SHA named in the script, with `git show`, from a Mole checkout (default `../mole`).

```sh
scripts/gen-bundle-globs.sh            # write the file
scripts/gen-bundle-globs.sh --check    # exit 1 when the committed file is stale
```

To take a newer Mole, change `MOLE_SHA` in the script and run it again. The script stops on a pattern that uses glob syntax other than `*`.

## Open-handles check

`scripts/open-handles-check.sh` tests one claim on the machine it runs on: a cache folder in which a process holds a file open never reads `Eligible`. It is macOS only. It reads the live machine with `lsof`, so two runs can list different folders, and it deletes nothing.

```sh
cargo build --release
scripts/open-handles-check.sh                      # target/release/dustpan
scripts/open-handles-check.sh path/to/dustpan out  # another binary, and keep the two lists in out/
```

It prints the number of held folders and each one that reads `Eligible`, and exits 1 when there is one. Before the guard read open handles, 7 of 41 held folders on the bench host read `Eligible` (`docs/design/m0-3-guard.md`, "Fix round 3").
