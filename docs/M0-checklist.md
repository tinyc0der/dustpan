# M0 foundations plan

M0 builds the parts of dustpan that every later milestone stands on. These are the workspace and CI on three OSes, five spikes, the core types and guard pipeline, config and the whitelist, the delete and Trash sinks with their logs, the TUI frame, and a parity harness against Mole. M0 deletes nothing a user asked for. When M0 is done, `dustpan` opens a working Home screen, and `--help`, `--version`, `explain` and `history` work. `explain PATH` is a small dustpan addition that prints the guard verdict for any path. It answers "why was this kept?" for users and gives the safety core a surface live lanes can drive.

The rule the program enforces is that no code reaches a sink without passing the guard, and every sink call writes a log row.

PRs in order are M0-1, M0-2, M0-3, M0-4, M0-5, M0-6, M0-7. `docs/PLAN.md` holds the product plan these PRs serve.

## How to read this

One box is one unit of work. Every box names the evidence that checks it. A nested box is a sub-step of the box above it. Check a box only when its evidence exists, a file, a log line, a screenshot, a test run, or a SHA. The body is a how-to. The appendices explain and record.

The program runs `skills/poteto-mode/playbooks/autopilot-stack.md` from the installed plugin. The operator lands every PR. M0-2 and M0-6 are the operator's items. They stop at merge-ready for a review in chat.

Tests alone are not sufficient verification. A PR is verified only when its unit, live, and perf boxes are all checked.

Dustpan is greenfield, so no PR has a trunk version of its feature. Each lane marked `Regression lane against trunk.` records that fact and gates the behavior the diff adds. Where Mole has the same behavior, the lane runs Mole as the reference instead.

## Program checklist

### Arm the program

- [ ] State the protocol and this plan to the operator, then stop. Start execution only on the operator's explicit go.
- [ ] On the operator's go, write this exact text into `notes.md` in the program's store under `~/.claude/orchestrate/dustpan-m0/`, and restate it in your todolist. "Plan `docs/M0-checklist.md`. PRs M0-1 to M0-7 in order. A PR is verified only when its unit, live and perf boxes are checked. The operator lands every PR. Done when M0-7 is merged and `docs/PLAN.md` Appendix A records a verdict for S1 to S5."
- [ ] Read these from the installed plugin at program start. Re-read them at every tick.
  - [ ] `skills/poteto-mode/playbooks/autopilot-stack.md`
  - [ ] `skills/swarm/SKILL.md`
  - [ ] The `run` skill until M0-6 lands, then `.claude/skills/verify/SKILL.md` in the dustpan repo.
  - [ ] `skills/poteto-mode/playbooks/opening-a-pr.md`
  - [ ] `skills/architect/SKILL.md`, `skills/interrogate/SKILL.md`, `skills/tdd/SKILL.md`
- [ ] Arm the hourly audit tick as a real `/loop` in dynamic mode, which schedules its own wake-up rather than blocking on a sleep. Never leave the cadence to memory.
- [ ] Use this tick prompt, verbatim. "Re-read the execution playbook from the installed plugin and the objective in the program store's `notes.md`. Audit the operation against both and fix drift in this tick. Probe every active lane and judge progress by side effects only. Stand down a stuck lane and dispatch its replacement now. Then post a short status message to the operator in chat only when the audit found a tracked change that no earlier status message reported, such as a PR opened, a code-ready head, a round launched or closed, a verdict, a merge, a stuck agent and the action taken, a blocker added or cleared, or a decision only the operator can make. Name every such change and nothing else. Do not repeat a table, the merged list, or an unchanged blocker. If the audit found none, end the turn with no reply text. Either way, log this tick's row in your decision trail. The row names the items reported, or none."
- [ ] On the operator's hold or stand-down, send every owner a zero-writes order at once.

### Spawn owners

- [ ] Spawn one owner per PR with the full lifecycle the execution playbook names.
- [ ] Follow this dependency graph. Start dependent work only after its parent merges, or base it on the parent branch when the execution playbook stacks.
  - [ ] M0-1 and M0-2 are independent and first. M0-1 branches from `main`. M0-2's spike branches are throwaway, and only its notes merge.
  - [ ] M0-3 after M0-1.
  - [ ] M0-4 after M0-3.
  - [ ] M0-5 after M0-4 and after M0-2 records the S2 verdict.
  - [ ] M0-6 after M0-1 and after M0-2 records the S4 verdict.
  - [ ] M0-7 after M0-3 and after M0-2 records the S5 verdict.
- [ ] Hold the file boundaries. M0-2 touches only `docs/spikes/**` and Appendix A of `docs/PLAN.md`. M0-6 touches only `crates/dustpan/src/tui/**` and `.claude/skills/verify/**`. M0-7 touches only `tests/parity/**` and `scripts/parity.sh`.
- [ ] Hold the review gate. M0-2 and M0-6 change what the operator decides or sees. They wait for the operator's review in chat before merge.

### PR mechanics, for every PR

- [ ] Resolve the forge once. Default to `gh`; if `command -v origin` succeeds and Origin can resolve the repository, use `origin pr` for every PR operation. Record any fallback to `gh`. Never require `gt`.
- [ ] Open the PR ready, never draft, per **Opening a PR**. Use the run's built-in PR tool when it has one, else `origin pr create --status open --base <base-branch>` or `gh pr create --base <base-branch>` according to the resolved forge. A stack child targets its parent branch.
- [ ] Run `cargo fmt --check`, `cargo clippy --workspace --all-targets -- -D warnings` once before the PR-facing push. Push with hooks on.
- [ ] Run `/deslop` before each commit and `/no-comments` before review.
- [ ] Triage every review-bot and security-reviewer comment per `references/bugbot-triage.md` in the poteto-mode skill.
- [ ] Before the code-ready report and babysit, and again after merge prep, record the base and head SHAs prepared by the topology owner under the execution playbook.

### Verdict and merge, for every PR

- [ ] A full swarm runs once, at the code-ready head. Another full swarm runs only for new behavior, a redesign, or a large new diff. A rebase, a merge-conflict resolution, or a minor fix gets a single verifier at the new head, and a wording-only change gets none. Run each full swarm per `skills/swarm/SKILL.md`. One gates lane. The ten live lanes from the PR's **Verify, live** block. The perf lane from its **Verify, perf** block. Two or more audit lanes, each with its own focus, that read the diff and the receipts and distrust the PR body. The root audits the receipts in the merge-ready report before the verdict.
- [ ] Judge every finding by the execution playbook's blocking rule. The verdict is clean when none blocks. A failure on trunk behavior in a lane marked `Regression lane against trunk.` is recorded against trunk and does not block the PR. A new head gets a fresh verdict, except for results that stay valid under the patch-id rule in `playbooks/shipping.md`.
- [ ] The root appends each verified PR to the stack, and the operator lands the stack bottom-up. A PR whose patch id is unchanged after a rebase keeps its verdict under the patch-id rule in `playbooks/shipping.md`.

### Boot recipe, for every live lane

Each live lane runs in its own worktree at the PR head. Drive through the skill path or exact commands recorded in this boot recipe.

- [ ] `git fetch origin <head-branch> && git checkout <head SHA>`.
- [ ] Run `cargo build --release`. Wait for `Finished`. For TUI lanes, start `target/release/dustpan` in a 120 by 40 pseudo-terminal through the `run` skill, or through `.claude/skills/verify/` once M0-6 lands.
- [ ] Deliver input only through the driver's key and command calls. Read-only diagnostics are `dustpan --version`, `dustpan explain --json`, and the files under the dustpan log folder.
- [ ] Save every capture to `/tmp/swarm-<pr-id>/worker-<n>/<slug>.png` for TUI lanes and `<slug>.txt` for CLI lanes, and return the paths with the report.
- [ ] Build fixture homes with `tests/fixtures/mkhome.sh <dir>` (added in M0-3) and run every lane that touches files with `HOME=<dir>`. No lane touches the real home folder.

## Create the workspace, CI and CLI skeleton (M0-1)

**Depends on.** None.

**Files.**

- [ ] Create `Cargo.toml` (workspace), `rust-toolchain.toml`, `crates/dustpan-core/`, `crates/dustpan-sys/`, `crates/dustpan/`.
- [ ] Create `.github/workflows/ci.yml` with jobs on `macos-14`, `macos-13`, `windows-latest`, `windows-11-arm`, `ubuntu-latest`.
- [ ] Create `LICENSE` (GPL-3.0-or-later unless the operator chose otherwise), `README.md`, `deny.toml`.

**Build.**

- [ ] `crates/dustpan/src/cli.rs` defines the `clap` command with `--version`, `--help`, `--debug`, `--ascii`, `--no-color`. A command appears in `--help` only once it works.
- [ ] `crates/dustpan/src/exit.rs` maps outcomes to the exit codes in `docs/PLAN.md` ("The CLI contract is the same on every OS").
- [ ] `--version` prints version, OS name and version, architecture, and install method, as Mole's CORE-04 does.
- [ ] `clippy.toml` lists `std::fs::remove_file`, `remove_dir`, `remove_dir_all` and `rename` under `disallowed-methods`, so only the future sink module can call them with an `#[allow]` (SAFE-01).
- [ ] `dustpan` exits 3 with a message when it runs as root (SAFE-16).

**You see.**

- [ ] `dustpan --version` prints `dustpan 0.1.0` and a second line such as `macOS 15.x · arm64 · cargo`.
- [ ] CI is green on all five runners.

**Verify, unit.** Tests alone are not sufficient verification. A PR is verified only when its unit, live, and perf boxes are all checked.

- [ ] `crates/dustpan/tests/cli.rs` gains cases for `--version`, `--help`, an unknown command (exit 2), and an unknown flag (exit 2). Run `cargo test --workspace`.

**Verify, live.** Tests alone are not sufficient verification. A PR is verified only when its unit, live, and perf boxes are all checked. Ten lanes on the configured `swarm workers` model at the PR head, per the boot recipe.

- [ ] Lane 1. Regression lane against trunk. Trunk has no dustpan. Record that, then run `mo --version` and `dustpan --version` and compare the fields. Save `version-vs-mole.txt`. Pass when dustpan prints version, OS, arch and install method.
- [ ] Lane 2. Run `dustpan --help`. Save `help.txt`. Pass when only working commands are listed.
- [ ] Lane 3. Run `dustpan frobnicate; echo $?`. Save `unknown-cmd.txt`. Pass when stderr names the command and the code is 2.
- [ ] Lane 4. Run `dustpan --frob; echo $?`. Save `unknown-flag.txt`. Pass when the code is 2.
- [ ] Lane 5. Run `NO_COLOR=1 dustpan --help | cat -v`. Save `no-color.txt`. Pass when the output has no escape bytes.
- [ ] Lane 6. Run `dustpan --version | cat`. Save `pipe.txt`. Pass when the output has no escape bytes and the exit code is 0.
- [ ] Lane 7. Run `dustpan --debug --version` and list the log folder. Save `debug.txt`. Pass when a debug session log exists and names no user or host.
- [ ] Lane 8. Operator-run on a Windows machine. Download the Windows x64 CI artifact and run `dustpan.exe --version` in Windows Terminal. Save `win-version.png`. Pass when the second line names Windows and `x86_64`.
- [ ] Lane 9. CI-run on `ubuntu-latest`. Download the Linux CI artifact and run `dustpan --version` in a container. Save `linux-version.txt`. Pass when the second line names the distro from `/etc/os-release`.
- [ ] Lane 10. Operator-run, because it needs a sudo password. Run `sudo dustpan --version`. Save `root.txt`. Pass when dustpan refuses to run as root with a message and exit 3, as Mole does (`mole:10-13`).

**Verify, perf.** Tests alone are not sufficient verification. A PR is verified only when its unit, live, and perf boxes are all checked.

- [ ] Metric. Wall time of `--version`. Trunk has no dustpan, so `mo --version` is the reference and dustpan also gets an absolute budget.
- [ ] Probe. `hyperfine --warmup 3 --runs 30 'mo --version' 'target/release/dustpan --version'`, both commands in one interleaved run.
- [ ] Baseline. Record the `mo --version` median first.
- [ ] Rule. Fail if the dustpan median exceeds 20 ms.

**Review gate.** None. M0-1 is not review-gated.

**Merge.**

- [ ] Root's clean verdict at the exact head SHA.
- [ ] Bugbot triage done.
- [ ] Base and verdict are current under the execution playbook and the patch-id rule in `playbooks/shipping.md`.
- [ ] The root appends M0-1 to the stack, and the operator lands it.

## Run spikes S1 to S5 and record each verdict (M0-2)

**Depends on.** None.

**Files.**

- [ ] Create `docs/spikes/s1-scan.md`, `s2-trash.md`, `s3-privilege.md`, `s4-terminal.md`, `s5-parity.md`.
- [ ] Edit `docs/PLAN.md` Appendix A to add the verdict, branch and SHA for each spike.

**Build.**

- [ ] S1. On branch `spike/s1-scan`, time a `jwalk` plus `getattrlistbulk` walk against `du -skPx` workers on `~`, five runs each, and compare byte totals per top-level entry.
- [ ] S2. On branch `spike/s2-trash`, move a file, a folder, an app bundle, and an item under `~/Library/Containers` to the Trash with each of the four methods in Appendix A, locally and over SSH. Record which ones support Put Back.
- [ ] S3. On branch `spike/s3-privilege`, suspend a `ratatui` screen, run `sudo -v` with Touch ID enabled, start `--privileged-helper` over a pipe, delete one root-owned fixture, and resume. Repeat with `runas` on Windows.
- [ ] S4. On branch `spike/s4-terminal`, render the frame mockup from `docs/PLAN.md` in the six terminals listed in Appendix A.
- [ ] S5. On branch `spike/s5-parity`, run `mo clean --dry-run`, `mo purge --dry-run --yes` and `mo installer --dry-run` with `HOME` set to a fixture folder, then parse the candidate lists.

**You see.**

- [ ] Five spike notes, each ending in one line `Verdict.` followed by the decision it makes.

**Verify, unit.** Tests alone are not sufficient verification. A PR is verified only when its unit, live, and perf boxes are all checked.

- [ ] Each spike branch keeps its measuring script under `spikes/<n>/`. Run each script twice and confirm the second run reproduces the first within 10 percent.

**Verify, live.** Tests alone are not sufficient verification. A PR is verified only when its unit, live, and perf boxes are all checked. Ten lanes on the configured `swarm workers` model at the PR head, per the boot recipe.

- [ ] Lane 1. Regression lane against trunk. Trunk has no scanner. Rerun the S1 script with Mole's `mo analyze --json ~` as the reference. Save `s1-rerun.txt`. Pass when the recorded totals reproduce within 1 percent.
- [ ] Lane 2. Operator-run on a second Mac. Rerun S2 there. Save `s2-rerun.txt`. Pass when the chosen method works locally and over SSH.
- [ ] Lane 3. Rerun S2 for an item under `~/Library/Containers`. Save `s2-containers.png`. Pass when the item appears in the Trash and the source is gone.
- [ ] Lane 4. Operator-run, because it needs a fingerprint. Rerun S3 with Touch ID. Save `s3-touchid.png`. Pass when one fingerprint prompt appears and the TUI resumes with no stray input.
- [ ] Lane 5. Operator-run, because it needs a password. Rerun S3 with a password and a wrong first attempt. Save `s3-password.png`. Pass when the retry works and no password character reaches the TUI.
- [ ] Lane 6. Operator-run on a Windows machine, because it needs a UAC click. Rerun S3 on Windows. Save `s3-windows.png`. Pass when one UAC prompt appears and the helper exits after its action.
- [ ] Lane 7. Operator-run on a Windows machine. Rerun S4 in Windows Terminal and conhost. Save `s4-windows.png`. Pass when bars and borders align or the recorded ASCII fallback applies.
- [ ] Lane 8. Rerun S4 in Ghostty and Terminal.app. Save `s4-macos.png`. Pass when the frame matches the mockup.
- [ ] Lane 9. Rerun S5 against a second fixture home. Save `s5-rerun.txt`. Pass when the parser reads all three commands' candidate lists.
- [ ] Lane 10. Read each spike note and its script. Save `audit.txt`. Pass when every number in a note comes from a script output in the branch.

**Verify, perf.** Tests alone are not sufficient verification. A PR is verified only when its unit, live, and perf boxes are all checked.

- [ ] Metric. S1 scan wall time on `~` for both methods, with byte totals per entry. This PR adds no shipped code, so S1's numbers are the perf evidence.
- [ ] Probe. The S1 script, interleaving `du` workers and the native walk, five runs each with the analyzer cache cleared.
- [ ] Baseline. Record Mole's `mo analyze --json ~` wall time first.
- [ ] Rule. The spike passes when the chosen method is within 1.2 times Mole's time and within 1 percent of its totals. Otherwise the verdict records the gap and M1's budget uses it.

**Review gate.** The operator reviews before merge.

- [ ] Copy lane 4, 6 and 8 screenshots into `docs/spikes/media/M0-2-review-<slug>.png`.
- [ ] Record a 30 to 60 second video of S3 on macOS. Save it as `docs/spikes/media/M0-2-review.mp4`.
- [ ] Post the screenshots, the video and the five verdicts in chat. Stop at merge-ready. Wait for the operator's click.

**Merge.**

- [ ] Root's clean verdict at the exact head SHA.
- [ ] Bugbot triage done.
- [ ] Base and verdict are current under the execution playbook and the patch-id rule in `playbooks/shipping.md`.
- [ ] The operator lands M0-2. Spike branches stay unmerged.

## Build the core types, guard pipeline and `explain` (M0-3)

**Depends on.** M0-1.

**Files.**

- [ ] Create `crates/dustpan-core/src/{path.rs,identity.rs,size.rs,candidate.rs,guard.rs,protect.rs}`.
- [ ] Create `crates/dustpan-sys/src/{macos,windows,linux}/identity.rs`.
- [ ] Create `crates/dustpan/src/cmd/explain.rs`, `tests/fixtures/mkhome.sh`.

**Build.**

- [ ] Run the `architect` skill on the types in `docs/PLAN.md` ("Types make an unguarded delete impossible") before writing code.
- [ ] `guard::evaluate(&AbsPath) -> Verdict` runs layers 1 to 4 from `docs/PLAN.md`. The protected lists come from `docs/inventory/safety-invariants.md` sections 2.1 to 2.5.
- [ ] `guard::recheck(Candidate) -> Result<Approved, KeptReason>` compares `PathIdentity` and reruns the layers.
- [ ] `dustpan explain PATH... [--json]` prints the verdict and the layer that decided it.

**You see.**

- [ ] `dustpan explain /System` prints `Kept · critical system path`.
- [ ] `dustpan explain ~/Library/Caches/com.example.app` prints `Eligible`.

**Verify, unit.** Tests alone are not sufficient verification. A PR is verified only when its unit, live, and perf boxes are all checked.

- [ ] `crates/dustpan-core/tests/guard.rs` gains one case per path in `safety-invariants.md` sections 2.1 to 2.4, plus case aliases and symlink aliases. Run `cargo test -p dustpan-core`.
- [ ] `crates/dustpan-core/tests/recheck.rs` gains a swapped-inode case and a symlinked-parent case that both refuse. Run `cargo test -p dustpan-core`.

**Verify, live.** Tests alone are not sufficient verification. A PR is verified only when its unit, live, and perf boxes are all checked. Ten lanes on the configured `swarm workers` model at the PR head, per the boot recipe.

- [ ] Lane 1. Regression lane against trunk. Trunk lacks `explain`. Record that, then run `dustpan explain` on every path in Mole's analyze protection list (`mac-analyze-status-tui.md` section 2.8, step 5). Save `analyze-list.txt`. Pass when every path is kept.
- [ ] Lane 2. Run `dustpan explain / /System /usr /Library /Applications`. Save `roots.txt`. Pass when all are kept as critical.
- [ ] Lane 3. Run `dustpan explain /SYSTEM ~/library`. Save `case-alias.txt`. Pass when both are kept as aliases of protected paths.
- [ ] Lane 4. In a fixture home, symlink `~/link` to `/System` and run `dustpan explain ~/link/Library`. Save `symlink.txt`. Pass when it is kept for a symlinked ancestor.
- [ ] Lane 5. Run `dustpan explain ~/Documents ~/Desktop ~/Downloads`. Save `home-heads.txt`. Pass when the folders are kept and a file inside each is eligible.
- [ ] Lane 6. Run `dustpan explain ~/Library/Containers/com.docker.docker`. Save `docker.txt`. Pass when it is kept.
- [ ] Lane 7. Run `dustpan explain $'~/bad\nname'`. Save `control.txt`. Pass when it is kept for control bytes and the name prints escaped.
- [ ] Lane 8. Run `dustpan explain --json ~/Library/Caches/x | jq .`. Save `json.txt`. Pass when the object has `path`, `verdict`, `layer` and `reason`.
- [ ] Lane 9. Run `dustpan explain relative/path`. Save `relative.txt`. Pass when it resolves to an absolute path or refuses with exit 2.
- [ ] Lane 10. CI-run on `windows-latest`. Run lanes 2 and 5 on the Windows CI artifact with `C:\Windows` and `%USERPROFILE%\Documents`. Save `windows.txt`. Pass when the same verdicts apply. Until M8 lists open files on Windows, the file inside `Documents` is kept as `file-open-unknown` and not eligible (`docs/design/m0-3-guard.md`, "Fix round 3").

**Verify, perf.** Tests alone are not sufficient verification. A PR is verified only when its unit, live, and perf boxes are all checked.

- [ ] Metric. Guard evaluations per second over 100,000 fixture paths. Trunk lacks the guard, so this sets an absolute budget.
- [ ] Probe. `cargo bench -p dustpan-core --bench guard`, three runs.
- [ ] Baseline. Record the first run's median as the M0 baseline in `docs/perf.md`.
- [ ] Rule. Fail below 200,000 evaluations per second, because a home folder scan can produce millions of candidates.

**Review gate.** None. M0-3 is not review-gated.

**Merge.**

- [ ] Root's clean verdict at the exact head SHA.
- [ ] Bugbot triage done.
- [ ] Base and verdict are current under the execution playbook and the patch-id rule in `playbooks/shipping.md`.
- [ ] The root appends M0-3 to the stack, and the operator lands it.

## Load config and the whitelist (M0-4)

**Depends on.** M0-3.

**Files.**

- [ ] Create `crates/dustpan-core/src/config.rs`, `crates/dustpan-core/src/whitelist.rs`.
- [ ] Edit `crates/dustpan-core/src/guard.rs` to add layer 5.

**Build.**

- [ ] One path module names the config, cache and log folders per OS.
- [ ] `whitelist::load` expands `~`, skips `#` comments, merges the hard-safety patterns, and tells a missing file (ENOENT) from an unreadable one. An unreadable file stops any destructive run, as Mole's WL-01 does.

**You see.**

- [ ] `dustpan explain ~/.gradle/caches/x` prints `Kept · protected by default whitelist`.

**Verify, unit.** Tests alone are not sufficient verification. A PR is verified only when its unit, live, and perf boxes are all checked.

- [ ] `crates/dustpan-core/tests/whitelist.rs` gains cases for `~` expansion, comments, globs, a missing file, an unreadable file, and a user file that omits a hard-safety pattern. Run `cargo test -p dustpan-core`.

**Verify, live.** Tests alone are not sufficient verification. A PR is verified only when its unit, live, and perf boxes are all checked. Ten lanes on the configured `swarm workers` model at the PR head, per the boot recipe.

- [ ] Lane 1. Regression lane against trunk. Trunk explains without layer 5. Run `dustpan explain` on the 16 default whitelist paths from `mac-optimize-core-cli.md` section 5 at trunk and head. Save `defaults.txt`. Pass when head keeps all 16.
- [ ] Lane 2. Add `~/Projects/keep` to the whitelist file and explain a child. Save `user-entry.txt`. Pass when it is kept with the file path named.
- [ ] Lane 3. Make the whitelist unreadable with `chmod 000`. Save `unreadable.txt`. Pass when `explain` says the whitelist cannot be read and exits 1.
- [ ] Lane 4. Delete the whitelist file. Save `missing.txt`. Pass when defaults apply.
- [ ] Lane 5. Write a whitelist that omits a hard-safety pattern. Save `hard-safety.txt`. Pass when that path is still kept.
- [ ] Lane 6. Add a glob `~/Library/Caches/com.foo.*` and explain `com.foo.bar` and `com.foobar`. Save `glob.txt`. Pass when only `com.foo.bar` is kept.
- [ ] Lane 7. Add a line with trailing spaces and a CRLF ending. Save `crlf.txt`. Pass when the entry still matches.
- [ ] Lane 8. Set `XDG_CONFIG_HOME` on Linux and run lane 2. Save `xdg.txt`. Pass when the file is read from that folder.
- [ ] Lane 9. CI-run on `windows-latest`. Run lane 2 on Windows with `%USERPROFILE%\Projects\keep`. Save `windows.txt`. Pass when it is kept.
- [ ] Lane 10. Run `dustpan explain --json` on a whitelisted path. Save `json.txt`. Pass when `layer` is `whitelist` and `source` names the file.

**Verify, perf.** Tests alone are not sufficient verification. A PR is verified only when its unit, live, and perf boxes are all checked.

- [ ] Metric. Guard evaluations per second with a 500-line whitelist, at trunk (M0-3 head) and at this head.
- [ ] Probe. `cargo bench -p dustpan-core --bench guard`, alternating trunk and head builds, three runs each.
- [ ] Baseline. Record the trunk median first.
- [ ] Rule. Fail if the head median is more than 25 percent slower than trunk.

**Review gate.** None. M0-4 is not review-gated.

**Merge.**

- [ ] Root's clean verdict at the exact head SHA.
- [ ] Bugbot triage done.
- [ ] Base and verdict are current under the execution playbook and the patch-id rule in `playbooks/shipping.md`.
- [ ] The root appends M0-4 to the stack, and the operator lands it.

## Add the delete and Trash sinks, the logs and `history` (M0-5)

**Depends on.** M0-4, and the S2 verdict from M0-2.

**Files.**

- [ ] Create `crates/dustpan-core/src/{sink.rs,oplog.rs,history.rs}`.
- [ ] Create `crates/dustpan-sys/src/{macos,windows,linux}/{delete.rs,trash.rs}`.
- [ ] Create `crates/dustpan/src/cmd/history.rs`.

**Build.**

- [ ] `Sink::Delete` takes only `Approved`. On Unix it opens the parent with `O_NOFOLLOW | O_DIRECTORY`, checks identity, and deletes with `unlinkat`. It never crosses a device boundary.
- [ ] `Sink::Trash` uses the method S2 chose, and `Trashed` sizes are never reported as freed.
- [ ] `oplog` writes Mole's row grammar with control bytes escaped, rotates at 5 MB, and warns once when the log cannot be written.
- [ ] `dustpan history [--json] [--limit N]` reads the logs.

**You see.**

- [ ] After an integration test run, `dustpan history` lists one session with its removed and kept rows.

**Verify, unit.** Tests alone are not sufficient verification. A PR is verified only when its unit, live, and perf boxes are all checked.

- [ ] `crates/dustpan-core/tests/sink.rs` gains cases for an inode swapped after approval, a parent replaced by a symlink, a mount point inside the target, and a missing path (success). Run `cargo test --workspace`.
- [ ] `crates/dustpan-core/tests/oplog.rs` gains cases for escaping, rotation, and an unwritable log folder. Run `cargo test -p dustpan-core`.

**Verify, live.** Tests alone are not sufficient verification. A PR is verified only when its unit, live, and perf boxes are all checked. Ten lanes on the configured `swarm workers` model at the PR head, per the boot recipe.

- [ ] Lane 1. Regression lane against trunk. Trunk has no sink. Run `cargo test --test sink_fixture` at head on a fixture home, then `dustpan history`, then `mo history` on a Mole log with the same rows. Save `history-vs-mole.txt`. Pass when both list the same session shape.
- [ ] Lane 2. Run `dustpan history --json | jq .`. Save `json.txt`. Pass when the keys match Mole's HIST-02 fields.
- [ ] Lane 3. Run `dustpan history --limit 0` and `--limit 201`. Save `limit.txt`. Pass when both exit 2.
- [ ] Lane 4. Append a row with invalid UTF-8 to the log and run `--json`. Save `utf8.txt`. Pass when the bad bytes read as U+FFFD and the log file is unchanged.
- [ ] Lane 5. Trash a fixture folder through the sink test on macOS, then open Finder's Trash. Save `trash.png`. Pass when the folder is there.
- [ ] Lane 6. Repeat lane 5 over SSH. Save `trash-ssh.txt`. Pass when the move succeeds with no Finder dialog.
- [ ] Lane 7. Mount a disk image inside a fixture folder and run the delete sink test on the folder. Save `mount.txt`. Pass when the mounted files survive.
- [ ] Lane 8. Make the log folder read-only and run the sink test. Save `readonly-log.txt`. Pass when one warning prints and the delete still happens.
- [ ] Lane 9. Operator-run on a Windows machine. Run the sink test on Windows. Save `windows.png`. Pass when the folder is in the Recycle Bin.
- [ ] Lane 10. Run the sink test on Linux. Save `linux.txt`. Pass when the item and its `.trashinfo` are in `~/.local/share/Trash`.

**Verify, perf.** Tests alone are not sufficient verification. A PR is verified only when its unit, live, and perf boxes are all checked.

- [ ] Metric. Time to delete a fixture tree of 100,000 files. Trunk has no sink, so `rm -rf` on the same tree is the reference and the sink gets an absolute budget.
- [ ] Probe. `cargo bench -p dustpan-sys --bench sink_delete`. The bench builds two identical 100,000-file trees in its own temp folder, times `/bin/rm -rf` on one and the sink on the other, alternates the order, and repeats ten times. It takes no path argument, so it cannot become the test-only delete command Appendix B rejects.
- [ ] Baseline. Record the `rm -rf` median first.
- [ ] Rule. Fail if the sink median exceeds 2 times the `rm -rf` median.

**Review gate.** None. M0-5 is not review-gated.

**Merge.**

- [ ] Root's clean verdict at the exact head SHA.
- [ ] Bugbot triage done.
- [ ] Base and verdict are current under the execution playbook and the patch-id rule in `playbooks/shipping.md`.
- [ ] The root appends M0-5 to the stack, and the operator lands it.

## Draw the TUI frame, Home and help overlay (M0-6)

**Depends on.** M0-1, and the S4 verdict from M0-2.

**Files.**

- [ ] Create `crates/dustpan/src/tui/{app.rs,frame.rs,home.rs,help.rs,theme.rs,keys.rs}`.
- [ ] Create `.claude/skills/verify/SKILL.md` with `/create-verification-skill`.

**Build.**

- [ ] The frame from `docs/PLAN.md` ("Every screen shares one frame"), with header, body, toast line and footer, plus priority-based hint dropping.
- [ ] Home lists the nine entries. Entries for later milestones show "Coming in Mn" and do not open.
- [ ] `?` opens the help overlay for the current screen. The terminal is restored on `q`, `Ctrl+C`, a panic, and SIGTERM.

**You see.**

- [ ] `dustpan` opens Home at 120 by 40 with the free space of the boot volume in the header.

**Verify, unit.** Tests alone are not sufficient verification. A PR is verified only when its unit, live, and perf boxes are all checked.

- [ ] `crates/dustpan/tests/tui_snapshots.rs` gains `insta` snapshots of Home at 60, 80 and 120 columns, and of the help overlay. Run `cargo test -p dustpan`.

**Verify, live.** Tests alone are not sufficient verification. A PR is verified only when its unit, live, and perf boxes are all checked. Ten lanes on the configured `swarm workers` model at the PR head, per the boot recipe.

- [ ] Lane 1. Regression lane against trunk. Trunk has no TUI. Open `mo` and `dustpan` side by side at 120 by 40. Save `home-vs-mole.png`. Pass when dustpan shows every Mole menu entry and the header shows free space.
- [ ] Lane 2. Press `↓` four times and `k` once. Save `nav.png`. Pass when the cursor is on entry 4.
- [ ] Lane 3. Press `?`. Save `help.png`. Pass when the overlay lists every Home key.
- [ ] Lane 4. Resize to 70 by 20. Save `narrow.png`. Pass when hints drop by priority and nothing wraps.
- [ ] Lane 5. Resize to 50 by 12. Save `too-small.png`. Pass when the minimum-size message shows and the frame returns after a resize to 120 by 40.
- [ ] Lane 6. Press `Ctrl+C`, then run `stty -a`. Save `restore.txt`. Pass when the terminal is back in cooked mode with the cursor shown.
- [ ] Lane 7. Send SIGTERM to the process. Save `sigterm.txt`. Pass when the terminal is restored.
- [ ] Lane 8. Run with `NO_COLOR=1` and with `--ascii`. Save `plain.png`. Pass when every state still has a symbol or word and no box-drawing glyph appears with `--ascii`.
- [ ] Lane 9. Operator-run on a Windows machine. Open Home in Windows Terminal. Save `windows.png`. Pass when the frame matches lane 1.
- [ ] Lane 10. Run `dustpan </dev/null | cat`. Save `no-tty.txt`. Pass when it prints help text and draws no TUI.

**Verify, perf.** Tests alone are not sufficient verification. A PR is verified only when its unit, live, and perf boxes are all checked.

- [ ] Metric. Time from process start to the first full frame. Trunk has no TUI, so `mo` with no arguments is the reference and dustpan gets an absolute budget.
- [ ] Probe. The verify skill's first-frame timer, ten runs each of `mo` and `dustpan`, interleaved.
- [ ] Baseline. Record the `mo` median first.
- [ ] Rule. Fail if the dustpan median exceeds 50 ms.

**Review gate.** The operator reviews before merge.

- [ ] Copy lane 1, 3, 4 and 9 screenshots into `docs/media/M0-6-review-<slug>.png`.
- [ ] Record a 30 to 60 second video of Home navigation, help, resize and quit. Save it as `docs/media/M0-6-review.mp4`.
- [ ] Post the screenshots and the video in chat. Stop at merge-ready. Wait for the operator's click.

**Merge.**

- [ ] Root's clean verdict at the exact head SHA.
- [ ] Bugbot triage done.
- [ ] Base and verdict are current under the execution playbook and the patch-id rule in `playbooks/shipping.md`.
- [ ] The operator lands M0-6 after the review.

## Build the Mole parity harness (M0-7)

**Depends on.** M0-3, and the S5 verdict from M0-2.

**Files.**

- [ ] Create `tests/parity/{fixture_home.rs,mole_runner.rs,diff.rs}`, `scripts/parity.sh`.

**Build.**

- [ ] `fixture_home` builds a home folder with caches, logs, browser profiles, apps, projects, installers, and symlink traps from a TOML manifest.
- [ ] `mole_runner` runs `mo clean --dry-run`, `mo purge --dry-run --yes` and `mo installer --dry-run` with `HOME` set to the fixture, and parses each candidate list.
- [ ] `scripts/parity.sh <command>` prints three lists, paths only Mole found, paths only dustpan found, and paths both found.

**You see.**

- [ ] `scripts/parity.sh clean` prints the Mole list and an empty dustpan list, because dustpan has no clean catalog until M3.

**Verify, unit.** Tests alone are not sufficient verification. A PR is verified only when its unit, live, and perf boxes are all checked.

- [ ] `tests/parity/parse.rs` gains cases built from saved Mole outputs for each of the three commands. Run `cargo test --test parity`.

**Verify, live.** Tests alone are not sufficient verification. A PR is verified only when its unit, live, and perf boxes are all checked. Ten lanes on the configured `swarm workers` model at the PR head, per the boot recipe.

- [ ] Lane 1. Regression lane against trunk. Trunk has no harness. Run `scripts/parity.sh clean` at head. Save `clean.txt`. Pass when the Mole list is non-empty and every path is inside the fixture home.
- [ ] Lane 2. Run `scripts/parity.sh purge`. Save `purge.txt`. Pass when the fixture's `node_modules` older than 7 days appears and a newer one does not.
- [ ] Lane 3. Run `scripts/parity.sh installer`. Save `installer.txt`. Pass when the fixture `.dmg` and installer zip appear and a plain zip does not.
- [ ] Lane 4. Run lane 1 twice. Save `repeat.txt`. Pass when both runs print the same lists.
- [ ] Lane 5. After lane 1, compare the fixture home with its manifest. Save `untouched.txt`. Pass when no file changed, which proves Mole's dry run wrote nothing.
- [ ] Lane 6. Check the real home's Mole logs before and after lane 1. Save `real-home.txt`. Pass when nothing under the real home changed.
- [ ] Lane 7. Add a whitelist entry to the fixture's Mole config and rerun lane 1. Save `whitelist.txt`. Pass when the entry's path leaves the Mole list.
- [ ] Lane 8. Point the harness at a missing `mo`. Save `no-mole.txt`. Pass when it exits 3 with an install hint.
- [ ] Lane 9. Run the harness in CI on `macos-14`. Save `ci.txt`. Pass when the job is green.
- [ ] Lane 10. Read the harness code. Save `audit.txt`. Pass when no code path runs Mole without `--dry-run`.

**Verify, perf.** Tests alone are not sufficient verification. A PR is verified only when its unit, live, and perf boxes are all checked.

- [ ] Metric. Wall time of `scripts/parity.sh clean` on the standard fixture. Trunk has no harness, so the metric gets an absolute budget.
- [ ] Probe. `hyperfine --runs 5 'scripts/parity.sh clean'`.
- [ ] Baseline. Record `mo clean --dry-run` alone on the same fixture first.
- [ ] Rule. Fail if the harness adds more than 5 seconds over that baseline, so it stays cheap enough to run on every catalog PR.

**Review gate.** None. M0-7 is not review-gated.

**Merge.**

- [ ] Root's clean verdict at the exact head SHA.
- [ ] Bugbot triage done.
- [ ] Base and verdict are current under the execution playbook and the patch-id rule in `playbooks/shipping.md`.
- [ ] The root appends M0-7 to the stack, and the operator lands it.

## Close the program

- [ ] Every box above is checked with its evidence.
- [ ] Reply to the operator with the report the execution playbook names.

## Appendix A. Prototype evidence

No prototype has run. The operator asked for the plan first. M0-2 answers S1 to S5 and records each branch, SHA and artifact here. Until then, every spike question in `docs/PLAN.md` Appendix A is unproven.

## Appendix B. Alternatives rejected

- **A hidden test-only delete command for live lanes.** It would give the sinks a CLI surface in M0, but a test seam that reaches a real sink is how Mole's incident I-64 happened. Live lanes drive the sinks through integration test binaries instead, and `explain` covers the guard.
- **Fold the spikes into the PRs that need them.** It would save one PR, but S2, S4 and S5 each gate a different later PR. One spike PR with an operator review settles them together.
- **Shell completion in M0.** `clap_complete` is cheap, but completions for commands that do not exist yet would mislead. It lands in M7.

## Appendix C. Risks

| Risk | PR | What the owner watches |
|---|---|---|
| S2 finds no Trash method that works over SSH and inside Containers | M0-5 | Fall back per item type, as Mole does, and record which type uses which method |
| S3 shows Touch ID and a TUI cannot share the terminal | M4 | Leave the alternate screen for the prompt, then redraw |
| `macos-13` runners retire | M0-1 | Cross-compile `x86_64-apple-darwin` on `macos-14` and test it under Rosetta |
| The parity harness reads the real home | M0-7 | Lane 6 and a CI check that `HOME` is under the temp folder |
| Mole's dry-run output format changes | M0-7 | Pin the Mole SHA the harness parses, and fail loudly on unknown lines |
| 12 lanes have no unattended driver. They need Touch ID, password or UAC prompts, `sudo`, a second Mac, or Windows desktop screenshots | M0-1, M0-2, M0-5, M0-6 | Each such lane is tagged "Operator-run" or "CI-run". The root batches operator-run lanes into one session per PR and posts the list in chat before the verdict |

## Appendix D. Links and reading list

- `docs/PLAN.md` for the product plan, the feature matrix and the TUI design.
- `docs/inventory/safety-invariants.md` before M0-3, M0-4 and M0-5.
- `docs/inventory/mac-analyze-status-tui.md` section 2.8 before M0-3.
- `docs/inventory/mac-optimize-core-cli.md` sections 5 and 6 before M0-4, M0-5 and M0-6.
- M0-3 and M0-5 get `skills/architect/SKILL.md` before code and `skills/interrogate/SKILL.md` before merge. M0-6 gets `/create-verification-skill`.
- The decision trail lives in `docs/decisions.md`, kept per `skills/show-me-your-work/SKILL.md`.
