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
