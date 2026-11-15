# S5: Mole parity harness

Spike S5 asks whether `mo clean`, `mo purge` and `mo installer` can run in dry-run mode against a fixture `HOME` and produce a candidate list a program can parse. The answer decides whether the M0-7 parity harness is a CI gate or a manual check.

- Branch: `spike/s5-parity` at `594db770b317c5ff0b177dbe029e423fea3f422b`.
- Scripts: `spikes/s5/mkfixture.sh` (the fixture), `deny-writes.sb` (the sandbox profile), `run.sh` (the run), `parse.py` (the parser), and `.github/workflows/s5-parity.yml` (the CI run).
- Outputs: `spikes/s5/results/run1/`, `run2/` (this Mac, Mole 1.63.0), `ci-macos-latest/` (GitHub runner, Mole 1.67.0), `ci-run.json` and `ci-run-summary.txt`.

## The fixture and the guard rails

`mkfixture.sh` builds a fake home under `$TMPDIR`. It has app and browser caches (Chrome, Firefox, a made-up app), logs, a file in `.Trash`, three projects (`node_modules` and `target` folders aged 30 days, and a fresh `node_modules`), and in `Downloads` a `.dmg`, a `.pkg`, a zip with an `.app` inside, and a plain zip. It also has symlink traps: two cache entries and a log entry that point outside the fixture, one of them at the real `~/Documents`, a `node_modules` symlink, and a `.dmg` symlink.

`run.sh` runs each command with `HOME` set to the fixture, stdin from `/dev/null`, and the `--dry-run` flag, inside `sandbox-exec -f deny-writes.sb`. The profile allows everything except writes under `/Users`, `/Library`, `/Applications`, `/opt` and `/usr/local`, and running `/usr/bin/sudo`. Before any `mo` run, the script proves the sandbox works (`sandbox-proof.txt`). A write under `/Users` fails with `Operation not permitted` (exit 1). Running `sudo` fails at `execvp` (exit 71). A write inside the fixture succeeds.

Before and after each command, the script lists the mtime and size of every file under the real `~/.config/mole`, `~/Library/Logs/mole` and `~/.cache/mole`. In both local runs and both CI runs, all three diffs are empty (`real-home-<command>-diff.txt`, and `real_home_changed no` in `runs.tsv`).

## Results

| Command | Wall time, run 1 | Wall time, run 2 | Exit | Candidates inside the fixture |
|---|---|---|---|---|
| `mo clean --dry-run` | 6.16 s | 5.91 s | 0 | 10 rows |
| `mo purge --dry-run --yes` | 0.76 s | 0.82 s | 0 | 2 rows |
| `mo installer --dry-run` | 0.41 s | 0.40 s | 0 | 3 rows |

`parse.py` checks 19 expectations against the fixture. In both local runs and both CI runs, 18 match (`candidates.tsv`):

- `clean` lists the Chrome and Firefox caches, the Chrome code and GPU caches, the app logs, the crash report and the `.Trash` file. It does not list the Chrome `Bookmarks` file, `Documents/notes.txt`, or anything behind the symlink traps.
- `clean` lists the symlink `Library/Logs/trap-logs` itself at 1 KB. It sizes the link and does not follow it.
- `purge` lists `Projects/oldrust/target` (2.1 MB) and `Projects/oldweb/node_modules` (1.1 MB). It leaves out the fresh `node_modules` and the symlinked one.
- `installer` lists `Example-1.0.dmg`, `Setup.pkg` and `ExampleInstaller.zip`. It leaves out the plain zip and the `.dmg` symlink.
- The one miss is `Library/Caches/com.example.notes`, a 3 MB cache folder that `clean` does not list. Mole 1.63.0 and 1.67.0 behave the same. The spike did not find why.

The second run reproduces the first. The candidate lists are identical after the generated names (an npm log timestamp and a Playwright profile suffix) are normalized.

## What leaks in from the host

A fixture `HOME` does not isolate everything:

- `clean-list.txt` has four rows outside the fixture on this Mac: old Chrome and Edge framework versions under `/Applications`, a Playwright profile, and the clang module cache under `/var/folders`. The CI run has one, the clang module cache.
- The `clean` stdout shows "Android system images · 34.00GB · ~/Library/Android/sdk/system-images" and the Docker daemon's storage. `ANDROID_HOME` and the Docker socket come from the host environment (`machine.tsv` lists the host variable names that reached `mo`).
- A dry run is not free of side effects. Each local run of the three commands added 974 files to the fixture, and each CI run added 960. Locally, 958 are under `Library/Caches/Homebrew` from `brew`; the rest are an npm debug log and Go telemetry files (`fixture-diff.txt`). The npm log then shows up as a `clean` candidate in the same run. No fixture file was removed or changed. The only other diff lines are four folder mtimes.

## Which output the parser reads

- For `clean`, read `$HOME/.config/mole/clean-list.txt`. It has one full path and size per row under `=== Section ===` headers. The stdout has only category totals.
- For `purge`, read the `✓ [DRY RUN] <path>, <size>` rows of stdout. Paths start with `~/`, which is the fixture `HOME`. Mole writes no list file.
- For `installer`, read the menu rows of stdout (`<name> <size> | <source>`), after stripping ANSI escapes and the `➤ ○` marks. Mole writes no list file, and the rows carry the base name and a source label, not the path. With stdin from `/dev/null`, the menu draws once and exits 0.

## CI

The workflow ran on two runner images (`ci-run.json`, `ci-run-summary.txt`):

- `macos-14` was cancelled within one second. GitHub reports "The configuration 'macos-14-arm64-us-default' is not supported".
- `macos-latest` is macOS 26.7.2 with Homebrew 7.0.11. `brew install mole` poured the `arm64_tahoe` bottle of Mole 1.67.0 in 7 seconds. Both runs passed with the same 18 of 19 expectations and an unchanged real home (`ci-macos-latest/runner.txt`, `run1/`, `run2/`).

The runner installed 1.67.0 while this Mac has 1.63.0, so a gate has to pin the Mole version it compares against. Mole ships often (`docs/PLAN.md` Appendix C).

## What M0-7 needs

1. Run on `macos-latest` and install a pinned Mole version.
2. Run every `mo` command with `--dry-run`, stdin from `/dev/null`, a fresh fixture `HOME`, a scrubbed environment (`env -i` with `PATH`, `HOME` and `TMPDIR`), and `deny-writes.sb`.
3. Compare only candidates inside the fixture, and treat other rows as host noise.
4. Keep a known-difference list. It starts with `com.example.notes`.

## Pending operator runs

None for S5. Lane 9 (a second fixture home) is a rerun of `run.sh`, which already builds a fresh fixture each time.

Verdict. Make parity a CI gate on `macos-latest`, not `macos-14`, which GitHub no longer runs; pin the Mole version, run each command with `--dry-run` against a fresh fixture `HOME` in the write-deny sandbox with a scrubbed environment, and parse `clean-list.txt` for `clean`, the `[DRY RUN]` stdout rows for `purge`, and the stdout menu rows for `installer`, comparing only paths inside the fixture.
