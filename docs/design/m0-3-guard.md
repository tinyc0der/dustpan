# M0-3 core types, guard pipeline and `explain`

This is the design the M0-3 arena chose (candidate 1), with the arena's synthesis and the differences the implementation accepted. The sections from Problem to Alternatives are the candidate's text and describe the sketch as it was judged. Where the shipped code differs, "Implementation reconciliation" says how.

## Problem

Dustpan's one rule is that nothing leaves the disk unless it passed one guard, and every deletion writes one log row. M0-3 builds the types every later milestone copies: the parsed path, file identity, the candidate, the verdict, the approval a sink accepts, and the guard that produces verdicts and approvals. It also ships `dustpan explain PATH`, which prints the guard's verdict and the layer that decided it.

Four constraints make the shape non-obvious.

1. `dustpan-core` makes no OS calls, yet layers 1 to 3 need the file system. They need ancestor symlinks (SAFE-05), aliases compared by file identity (SAFE-04), the Data-volume firmlinks (SAFE-03), owner processes (SAFE-08) and open SQLite companions (SAFE-09). Core unit tests must run all of these against a fake.
2. The guard must judge at least 200,000 paths per second, because a home scan yields millions of candidates.
3. A discovery snapshot is not a lease. The sink must judge again with fresh evidence (SAFE-10, SAFE-11), so nothing remembered during discovery may approve a deletion.
4. M0-4 (whitelist, layer 5), M0-5 (sinks, oplog), M3 (rules with owners), M4 (privileged helper) and M8 (Windows) must slot in without reshaping these types.

M0-1 ships `dustpan_core::paths::{Os, Env, home_dir}` and the binary's `Exit` enum. This design reuses both.

## Usage (caller's view)

Written first. The types below are derived from these four call sites, and `crates/dustpan/src/cmd/explain.rs` and `crates/dustpan-core/tests/{usage,guard,recheck}.rs` type-check them against the sketch.

**`dustpan explain` (M0-3, `crates/dustpan/src/cmd/explain.rs`).**

```rust
let probe = LiveProbe::new();                                    // dustpan-sys
let guard = Guard::new(GuardInputs { os, home, env }, &probe)?;  // Err: print it, exit 1
let mut pass = guard.pass(&probe);
for arg in args {
    let abs = if Path::new(arg).is_absolute() { arg.clone() } else { cwd.join(arg).into() };
    let (path, verdict) = match AbsPath::parse(&abs) {
        Ok(path) => { let j = pass.evaluate(path); (j.path().to_string(), j.verdict().clone()) }
        Err(e) => (escaped(&abs).to_string(), Verdict::Kept(KeptReason::Shape(ShapeReason::Malformed(e)))),
    };
    // text:  /System  Kept · critical system path
    // json:  {"path", "verdict": "kept", "layer": r.layer().code(), "reason": r.code(), "detail": r.to_string()}
}
```

**A scan (M1 analyze, M3 clean).** The walker is sys's (spike S1). It reads each directory once and reports every entry's type and id.

```rust
let mut pass = guard.pass(&probe);                         // one per worker thread
for entry in walker.walk(&root) {                          // depth-first
    match pass.candidate(entry.path, entry.node, &rule) {  // no guard syscall for the leaf
        Ok(c) => plan.push(c),                             // Eligible or Kept, both are review rows
        Err(judgment) => oplog.skipped(&judgment),         // the parent could not be observed
    }
}
```

**The executor and sinks (M0-5).** Dry run and real run share every step, including the recheck. Only the sink call differs (SAFE-18).

```rust
for c in plan {
    if cancel.is_set() { out.push((c, Outcome::Cancelled)); continue; }      // SAFE-20
    match guard.recheck(c, &probe) {                       // fresh pass, every layer, identity compare
        Ok(a) if mode == Mode::Apply => out.push(sinks.apply(a)),  // the only door to remove_*
        Ok(a) => { let c = a.into_candidate(); let s = c.size; out.push((c, Outcome::WouldRemove(s))) }
        Err(Refused { candidate, rejection: Rejection::Gone }) => out.push((candidate, Outcome::AlreadyGone)),
        Err(Refused { candidate, rejection: Rejection::Kept(r) }) => out.push((candidate, Outcome::Skipped(r))),
    }
}
```

**A core unit test that runs layers 1 to 3 against a fake.**

```rust
let fs = FakeProbe::new()
    .dir("/Users/alice/Library/Caches/com.google.Chrome")
    .alias("/SYSTEM", "/System")                  // case-insensitive APFS: same FileId
    .symlink("/Users/alice/link", "/System")
    .running(42, "/Applications/Google Chrome.app/Contents/MacOS/Google Chrome");
let guard = Guard::new(GuardInputs { os: Os::MacOs, home: p("/Users/alice"), env: &no_env }, &fs)?;
let mut pass = guard.pass(&fs);
assert_eq!(pass.evaluate(p("/SYSTEM/Library")).verdict().kept().map(KeptReason::layer), Some(Layer::Critical));
assert!(matches!(pass.evaluate(p("/Users/alice/link/Library")).verdict(),
                 Verdict::Kept(KeptReason::Shape(ShapeReason::LinkedAncestor { .. }))));
```

## Shape

### Data structures

| Type | File | What it guarantees |
|---|---|---|
| `AbsPath` | `crates/dustpan-core/src/path.rs` | Absolute, normalized, no `..`, no control byte, drive-letter only on Windows. Built only by `parse` and `join(&Name)`, so an empty variable can never collapse a path to a parent root (SAFE-02, SAFE-03, SAFE-04). |
| `Name` | `crates/dustpan-core/src/path.rs` | One directory entry name. Not empty, `.` or `..`, no separator. |
| `escaped(&OsStr)` | `crates/dustpan-core/src/path.rs` | The one escaping rule (`\n`, `\r`, `\t`, `\xHH`) for terminal, `--json` and log rows (SAFE-22). |
| `FileId { volume: u64, file: u128 }` | `crates/dustpan-core/src/identity.rs` | dev and ino on Unix. Volume serial and the 128-bit `FILE_ID_INFO` id on Windows, because ReFS Dev Drives need all 128 bits. |
| `Node { kind, id }`, `PathIdentity { parent, target }` | `crates/dustpan-core/src/identity.rs` | What `lstat` saw. The identity a candidate carries from discovery to the sink. |
| `Probe` trait, `Seen<T>`, `Unseen` | `crates/dustpan-core/src/probe.rs` | Four primitive, policy-free questions with tri-state answers. Implementations hold nothing between calls. |
| `Owner`, `OwnerState`, `ProcessTable`, `Handles` | `crates/dustpan-core/src/probe.rs` | Literal owner names (SAFE-33). `OwnerState::permits_removal` is the one translation of the tri-state (SAFE-08). Owner matching against the table is pure core code. |
| `Policy` (private) | `crates/dustpan-core/src/protect/` | One `Table` of rows per OS, compiled into three tries (critical, protection, carve-outs), an identity table and verified firmlinks. |
| `Guard`, `GuardInputs`, `PolicyError` | `crates/dustpan-core/src/guard.rs` | The only public face of the policy. Built by `Guard::new` or not at all. |
| `Pass`, `Judgment` | `crates/dustpan-core/src/guard/pass.rs` | A stack of ancestor frames shared by every path a pass judges, plus at most one process table. |
| `Verdict`, `KeptReason`, `Layer`, `Hit`, `Kind`, `ShapeReason`, `LiveReason` | `crates/dustpan-core/src/guard/verdict.rs` | Refusals as data. The outer `KeptReason` variant is the layer, so a reason cannot name the wrong layer. Stable codes for `--json` and the oplog. |
| `Rule`, `Sink`, `Candidate` | `crates/dustpan-core/src/candidate.rs` | A rule's shared facts (owners, sink). A candidate carries one discovery verdict and identity, built only by `Pass::candidate`. |
| `Size` | `crates/dustpan-core/src/size.rs` | `Known`, `Partial`, `Unknown`; unknown never renders as 0 (SAFE-21). |
| `Approved`, `Refused`, `Rejection` | `crates/dustpan-core/src/guard/approved.rs` | An approval only `Guard::recheck` builds. Not `Clone`, `Default` or `Deserialize`. |
| `LiveProbe` | `crates/dustpan-sys/src/probe.rs` | The real `Probe`, with `lstat`, `readlink` and a process table. `unix/identity.rs` is real code. Windows identity and the process tables are `todo!()` with the API named. |

### How core reaches the file system without OS calls

Core defines `Probe` with four questions: `node` (one `lstat`), `link_target` (one `readlink`), `processes` (one process-table snapshot) and `in_use` (are any of these files open, asked once for a SQLite family). Every decision about what to ask and what an answer means stays in core. Symlink resolution (`probe::physical`), owner matching (`Owner::state_in`), the SQLite family rule, firmlink mapping and alias detection are all pure core code over probe answers. `dustpan-sys` implements the four questions with syscalls and keeps no state, so every answer is fresh.

A core unit test passes `probe::fake::FakeProbe`, an in-memory tree where two spellings that share a `FileId` are aliases. It can model a case alias, a firmlink, a symlinked ancestor, an unreadable directory, a running owner, an unreadable process table, an open WAL file, and a swap between discovery and the sink. It also counts calls, so a test asserts ancestor sharing by what the probe saw, not by reading internals. The fake compiles only under the `testkit` feature, which core's integration tests turn on through a self dev-dependency. That dev-dependency is verified to compile. No shipped crate names `FakeProbe` or `testkit`, and M0-3 adds a source test that fails if one does (SAFE-17).

Core's integration tests and bench can also dev-depend on `dustpan-sys`. They link the same core library sys does, so `LiveProbe` satisfies core's `Probe` there. The bench compiles with `&LiveProbe` as a `&dyn Probe`, which proves the types unify. So M0-5's `crates/dustpan-core/tests/sink.rs` can drive the real sinks. Only `#[cfg(test)]` unit tests inside core would see two copies of core.

### Data flow

1. **Build.** `Guard::new(GuardInputs { os, home, env }, &probe)` selects the OS table and expands `~` and `%VAR%` anchors. It fails with `HomeIsCritical` when `HOME` is `/` or `/Users`. It inserts rows into the tries, and records the `FileId` of every anchored refuse row that exists, about 120 `lstat` calls once per process. An anchor that exists but cannot be inspected fails the build. It keeps each macOS firmlink only after `/<x>` and `/System/Volumes/Data/<x>` prove to be one directory.
2. **Judge.** `Pass::judge(path, seen, owners)` runs four layers in order, and the first refusal decides.
   - Layer 1, shape. The path is mapped to its firmlink spelling. The pass reuses frames for ancestors it already holds and `lstat`s only new ones. Any linked ancestor refuses (`LinkedAncestor`), and so does an unreadable one (`Unreadable`). A leaf whose volume differs from its parent's is a mount point (`MountPoint`, SAFE-38). A leaf link has its target resolved and judged by a nested pass, and that target can refuse but never grant (`LinkToProtected`, SAFE-05).
   - Layer 2, critical. The deepest matching anchor wins, carve-out rows join as allow rows, and refuse beats allow at equal depth. Identity rows only refuse. They compare the leaf with Exact and Subtree anchors, the parent with Children anchors, and every ancestor with Subtree anchors (already folded into its frame).
   - Layer 3, live. PowerLog `Current*` stores and EDR caches refuse by name. Owners come from the rule plus the bundle id of a user-cache scope, and are matched against the pass's single process table. A SQLite family or partial download is sent to `in_use`. These run even under carve-outs (SAFE-06).
   - Layer 4, protection. The same trie contest over the protection rows and bundle-id globs. An anchor marked `RefuseWithAncestors` refuses every ancestor while it exists, one `lstat` each (SAFE-27).
   - The identity returned is the parent frame's `FileId` and the leaf's.
3. **Recheck.** `Guard::recheck(candidate, &probe)` starts a fresh pass, judges the path with the rule's owners and a fresh `lstat` of the leaf, and compares parent and target identity with discovery's. It returns `Approved`, `Refused { Kept(reason) }` or `Refused { Gone }`. It never refreshes the identity to approve a replacement. Its body is real code, not pseudocode.

### Cost of one evaluation

A scanned candidate costs no leaf syscall, because `Pass::candidate` takes the `Node` the walker's directory read already reported. Ancestor directories cost one `lstat` each per pass. `explain` and `recheck` pay one leaf `lstat`. Policy matching costs no syscall. It takes one trie step per component and one hash lookup per `FileId`, with ancestor results cached in their frames. A process table is taken at most once per pass and only when a path needs an owner check.

I measured the syscall part with `crates/dustpan-sys/examples/probe_cost.rs` (real `LiveProbe::node`, read-only) over the first 100,000 depth-first paths of `~/Library/Caches`. It ran three invocations of five interleaved rounds on an M1 Pro, macOS 26.6.2, at load average 6.5 to 9 while other agents built in parallel.

| Mode | `lstat` per path | Median per path, three runs | Paths per second |
|---|---|---|---|
| walker-fed, ancestors shared (`Pass::candidate`) | 0.08 | 0.53 µs, 0.59 µs, 0.84 µs | 1.2 M to 1.9 M |
| leaf `lstat` too, ancestors shared (`Pass::evaluate`) | 1.08 | 5.5 µs, 33 µs, 67 µs | 15 k to 181 k |
| every ancestor of every path (no sharing) | 12.18 | 19 µs, 22 µs, 114 µs | 9 k to 52 k |

The limiter for the two leaf-`lstat` rows is the vnode cache, not the syscall itself. That attribution is an inference from counters, not a profile. `kern.num_vnodes` equals `kern.maxvnodes` (263,168) on this machine, so a first touch of a leaf recycles a vnode. On a 20,000-path subset that stays cached, a leaf `lstat` costs about 2.5 µs per path. So a design that `lstat`s every leaf misses the 5 µs budget on a loaded machine, whatever it shares. The walker-fed row leaves about 4 µs of the budget for policy CPU, which I estimate at under 1 µs per path (a guess until `Policy` exists). The walker-fed timing includes the example's frame bookkeeping allocations. Its limiter was not profiled.

The bench (`crates/dustpan-core/benches/guard.rs`) gates the scan-shaped number. It walks the fixture tree once outside timing to collect nodes, then times `Pass::candidate` with `LiveProbe` and fails below 200,000 per second. It also prints the `explain`-shaped number and a `FakeProbe` CPU-only number, each with its `lstat` count per path, so a regression can be traced to syscalls or to policy.

### Invariants encoded in types (per type-system-discipline)

- Only `Guard::recheck` builds an `Approved`. Its fields are private to `guard::approved`, and it is not `Clone`. The compile-fail doctests with codes E0451 and E0599 pin this. A sink signature that takes `Approved` by value means one approval, one sink call.
- No `Guard` exists without a compiled policy, and no `Approved` exists without a `Guard`. A failed load therefore refuses every path by construction (SAFE-07). There is no `Policy::unavailable()` value for a caller to mishandle.
- Every candidate was judged before any dry-run branch. `Candidate::new` is `pub(crate)` and only `Pass::candidate` calls it, so the verdict, identity and owners come from one evaluation (SAFE-18).
- A reason cannot disagree with its layer. `KeptReason`'s outer variant is the layer, and `layer()` and `code()` are exhaustive matches.
- `Sink` has no review-only variant, so `Approved::sink()` is total and no sink rejects a row. `OwnerCommand` and `PrivilegedAction` are uninhabited until M3 and M4 add variants, so no M0 code can route to them.
- Identity only ever refuses. Allow rows match by spelling alone, so an alias can refuse a path but never grant one.

### Validation lives at the boundaries (per boundary-discipline)

`AbsPath::parse` and `Name::parse` are the only path parsers. The CLI resolves a relative argument against the working folder before parsing, so the guard never sees one. `Guard::new` validates every table anchor once. Inside, the pass trusts `AbsPath` and never re-checks shape.

### What it deliberately does not do

- It does not cache across passes. A pass dies before the sink, and `recheck` builds its own (per separate-before-serializing-shared-state, nothing is shared between the discovery and sink actors).
- It does not resolve a linked ancestor into an eligible path. It refuses and names the physical spelling for `explain` to print.
- It does not size, walk trees, or delete. The walker and the sinks' pre-delete walk (M0-5) call into a pass.
- It does not handle cancellation. The executor checks its sticky token before each recheck (SAFE-20).

### Interface depth

The public surface is three calls (`Guard::new`, `Guard::pass`, `Guard::recheck`), two pass methods (`evaluate`, `candidate`), read-only accessors, and the verdict data types. Behind it sit four layers, three tries with nearest-anchor semantics, the identity table, firmlink mapping, physical resolution, owner matching, SQLite family logic, ancestor sharing and every tri-state rule. A caller never learns the layer order, the table format or the probe calls. The `Probe` trait is the one deliberate seam, kept to four primitive questions so the fake stays dumb and the policy lives in one place.

### How later milestones slot in

- **M0-4, whitelist.** `GuardInputs` gains a required `whitelist` field, so both construction sites (the binary and the test helper) must supply one. `Policy` gains a fourth trie built from the same row vocabulary, with whitelist entries guarding their ancestors (SAFE-11). `judge` gains a layer-5 step after layer 4, and `KeptReason` gains `Whitelisted(WhitelistHit)` carrying `source`, which `explain --json` prints. `Layer::Whitelist` and its `whitelist` code already exist. `recheck` picks the layer up with no change, because it runs `judge`. An unreadable whitelist fails before `Guard::new`, so `explain` exits 1 by the same path as a policy error.
- **M0-5, sinks and oplog.** Each sink takes `Approved` by value and reads `path()`, `identity()` and `sink()`. The oplog writes `KeptReason::code()` and `escaped()` paths. The executor maps `Rejection::Gone` to done. The sink's pre-delete walk uses a `Pass` to refuse a protected descendant.
- **M3, rules with owners.** The catalog loader builds `Arc<Rule>` and may add fields (roots, minimum age, label). `Owner` may gain variants, sys implements `in_use`, and `OwnerCommand` gains variants. Review-only rows become planner rows that never reach `Pass::candidate`.
- **M4, privileged helper.** `PrivilegedAction` gains variants. The helper deserializes candidates, builds its own `Guard` as root, and runs `recheck`. SAFE-14 needs each ancestor's owner and mode, so `Node` grows two fields then, which touches only the two probe implementations.
- **M8, Windows.** `protect/windows.rs` holds the rows and already compiles everywhere. `crates/dustpan-sys/src/windows.rs` fills `node` and `link_target`. No core type changes.

### Changes from PLAN's sketch, each justified

- `PathIdentity` drops `mtime`. A directory's mtime moves whenever an entry inside it changes, so an mtime-bound identity would refuse every cache a daemon touched during review. SAFE-10 asks for parent and target `(dev, ino)` only, and content age is SAFE-26's job in M3.
- `FileId.file` is `u128`. ReFS Dev Drives on Windows use 128-bit file ids.
- `Sink::ReviewOnly` is removed. Review-only rows (large files, snapshots, iOS backups) never touch the disk, so M3 models them as planner rows that never become candidates, and `Approved::sink()` needs no impossible arm.
- `Candidate.rule, group, sink, default_selected` move into a shared `Arc<Rule>`, which also carries `owners`. `recheck` and the M4 helper re-probe owners without a catalog lookup.
- `Candidate.why` is removed. The detail sentence is derived from the verdict and the rule (single source of truth).
- `Candidate`'s path, identity and verdict are private with accessors. Size and last-used stay public for the sizer.
- `OwnerState::Unknown` carries an `Unseen` cause, so the refusal can name it (SAFE-35).
- `guard::evaluate(&AbsPath)` from the checklist becomes `guard.pass(&probe).evaluate(path)`. A free function cannot share ancestor work, and the perf budget needs that sharing.
- `guard::recheck(Candidate) -> Result<Approved, KeptReason>` becomes `Guard::recheck(candidate, &probe) -> Result<Approved, Refused>`. The candidate comes back for the outcome row, and `Gone` is separate from a refusal because M0-5 counts a missing path as done.
- New files beyond the checklist's list are `crates/dustpan-core/src/probe.rs`, `crates/dustpan-core/src/protect/{table,compiled,macos,windows,linux}.rs` and `crates/dustpan-core/src/guard/{pass,verdict,approved}.rs`. `crates/dustpan-sys/src/unix/identity.rs` serves macOS and Linux, because both use the same `lstat`. Per-OS modules exist only where behavior differs (process tables, Windows identity).

### SAFE-05's positive control is changed

SAFE-05's test expects `tmp/link/file` to keep the verdict of `real/file`. This design refuses any linked ancestor outright, as `docs/PLAN.md` layer 1 and M0-3 live lane 4 require, and as Mole's Windows branch does. Three reasons decide it. The sink opens the parent with `O_NOFOLLOW`, so a path with a linked ancestor could never pass the sink anyway. A scan that resolved a redirected `~/Library/Caches -> ~/Documents` would offer the contents of Documents, which is exactly SAFE-05's threat. And the refusal names the physical path, so `explain` can tell the user what to judge instead. SAFE-05's other half stays. A leaf link's target is judged, and it can only refuse.

### Red-flag screen

- **Shallow module.** Not present. Each use (explain, scan, sink) is one call.
- **Information leakage.** Not present. `protect` is private, tables are data, and the trait carries no policy. Reason codes exist in one match. The binary assembles `--json` from core's codes, and M0-5 moves the envelope type into core, which PLAN assigns the JSON schema.
- **Temporal decomposition.** Avoided. The section 3.5 typestate (`Discovered`, `Filtered`, `Sized`, `Approved`) was rejected, and discovery and recheck share one `judge`.
- **Pass-through.** `Guard::new` and `Guard::pass` are thin, but they keep `Policy` private and host `recheck`. This is the one place a thin layer earns its keep.
- **Split ownership.** Not present. Each pass owns its frames and its process table. The identity table is written once, at build.
- **Two ways to do one task.** `evaluate` and `candidate` look alike but serve different tasks. One judges a bare path and takes its own `lstat`. The other builds a candidate from a rule and a walker's node. Neither can do the other's job.
- **Importable internals.** `protect`, `Policy`, `Approved`'s fields and `Candidate::new` are unreachable from other crates. `FakeProbe` is feature-gated.
- **Hand-synced list.** There is one table per OS, and carve-outs are one list applied to two layers. The macOS firmlink list copies `/usr/share/firmlinks`, but each entry is verified by identity at build, so drift fails closed. The ~480 bundle globs should come from a script over Mole's `app_protection_data.sh`, not a hand copy.

## Synthesis decision

### Base

Candidate 1 is the base. The root and the cross-judge picked it independently (judge totals: c1 28, c2 22, c3 19, c4 19 of 30; `judge.md`). It is the only candidate that measured its cost model. On 100,000 real paths under `~/Library/Caches`, a leaf `lstat` per path cost 5.5 to 67 microseconds under vnode-cache pressure, so the scan path takes the walker's `Node` (`Pass::candidate`) and the guard never stats the leaf there. It has the deepest surface: `Guard::new`, `Guard::pass`, `Guard::recheck`, `Pass::{evaluate, candidate}`. A failed policy build leaves no `Guard`, so SAFE-07 holds by construction. Candidate 1 read the rubric before writing; the judge checked its claims against its source and they held.

All four candidates converged on the same core: one OS-free probe trait in core, ancestor work shared per pass, a private-field `Approved` that only `recheck` builds, refusals as data with the layer derived, one table per OS, no layer that can allow, and a flat refusal of linked ancestors.

### Grafts

1. From candidate 3: `recheck` rebinds identity before and after the live probe and compares both with discovery's identity, never refreshing it (SAFE-10 order). Candidate 1 observed identity in layer 1 and compared after the owner probe, which leaves the probe window unchecked.
2. From candidate 3: a separate layer for refusals only `recheck` can produce (identity changed, path replaced). Named `Layer::Identity`, JSON `"identity"`. Candidate 1 filed these under shape.
3. From candidate 2: `KeptReason::source()` exists from M0-3 and `explain --json` always emits `source` (null until M0-4), so M0-4 does not change the JSON shape.
4. From candidate 4: `crates/dustpan-core/clippy.toml` with `disallowed-methods` for `std::fs::*` functions, `std::env::var`/`var_os`/`vars`, `std::process::Command::new` and `std::time::SystemTime::now`, so an OS call in core fails `clippy -D warnings`. A crate-level `clippy.toml` replaces the workspace one for that crate, so it repeats the four SAFE-01 bans.
5. From candidate 4: Windows identity uses `GetFileInformationByHandleEx(FileIdInfo)` (128-bit id) through `windows-sys`, because `MetadataExt::file_index` is unstable on Rust 1.92.

### Rejected grafts

- Candidate 2's two-syntax `AbsPath::parse(os, raw)`. It would let a macOS developer judge Windows rows locally, but CI already runs core's tests on `windows-latest` and `windows-11-arm` for every PR, and a second path grammar in core is parser surface the safety core does not need. `AbsPath` stays a host-syntax `PathBuf`.
- Candidate 4's link target in `PathIdentity`. `recheck` runs a fresh pass that resolves and judges the leaf link's current target, so a retargeted link is already re-judged.
- Candidate 2's `mtime` in `PathIdentity`. Three of four candidates drop it: a cache directory's mtime moves while daemons write, and content age is SAFE-26's job in M3.
- Candidate 3's `Eligible(Needs)` split, where `explain` never probes processes. `explain` should print what the sink would decide now, so it takes one process table like any pass.

### Decisions applied by default (operator may override)

- SAFE-05: every path below a symlinked ancestor is kept, as `docs/PLAN.md` layer 1 and M0-3 lane 4 say. The refusal names the physical spelling. Discovery resolves its roots physically (SAFE-29), and the binary resolves `HOME` physically before it builds the guard, so a symlinked home does not refuse every path.
- `explain --json` follows PLAN's envelope: one object with `schema_version`, `command`, `run_id`, `mode`, `items[]`; each item has `path`, `verdict`, `layer`, `reason`, `detail`, `source`.
- `explain` expands a leading `~/` with the physical `HOME`, resolves a relative path against the working folder, then parses. It exits 0 when every path was judged (kept or eligible), 1 when the policy cannot be built, 2 on usage errors.
- Sys gains a process table in M0-3 (macOS `proc_listallpids` plus `proc_pidpath`; Linux `/proc`; Windows may answer `Unseen` until M8), because the live-user-cache rule needs it for `explain ~/Library/Caches/com.example.app` to print `Eligible`.
- SAFE-38: a leaf on a different volume from its parent is kept as a mount point.
- Over-protection relative to Mole under carve-outs (a bundle glob beats `/private/var/folders`) is accepted; the parity gate fails only on extra candidates.

### Verification of the synthesis

The base sketch type-checks on three targets with clippy clean and three compile-fail doctests (candidate 1's report). The grafts change `recheck`'s order, add a layer variant, add one accessor, add a crate `clippy.toml` and fill the Windows identity call. None reshapes a public type. The implementing PR proves them with the checklist's unit tests, a recheck test that swaps the target during the owner probe, and live lanes.

## Tradeoffs accepted

- We accept a walker-supplied `Node` in the discovery verdict in exchange for no leaf syscall per scanned candidate. A wrong node can mislabel a review row, but it cannot approve anything, because `recheck` takes its own `lstat`.
- We accept refusing every path below a linked ancestor (`/tmp/x` on macOS, every home path when `HOME` sits under a symlink) in exchange for a sink that only ever sees physical paths. The refusal names the physical spelling.
- We accept an identity table taken once per process in exchange for zero per-path alias syscalls. An entry can only refuse, so staleness over-refuses. The one gap is an alias of an anchor created after start that string folding does not catch, such as a Unicode-normalization variant. It closes at the next run.
- We accept over-protecting relative to Mole under carve-outs. A bundle-id glob on a leaf beats a shallower carve-out such as `/private/var/folders`, where Mole skips its protection check. M3's parity gate fails only on extra candidates, not on missing ones.
- We accept `in_use` answering `Unknown` until M3. In M0, SQLite-family and partial-download paths show as kept with `file-open-unknown`. That fails closed.
- We accept sys implementing process tables in M0-3, which the checklist does not list. The live-user-cache rule (SAFE-06 order) needs one for `explain ~/Library/Caches/com.example.app` to print `Eligible`.

## Alternatives considered

- **Facts in, verdict out.** Sys gathers a `PathFacts` struct (ancestor `lstat`s, identities, owner states) and core judges plain data. It looks purer, but sys must know which facts each rule needs, so the policy leaks across the boundary. Every new rule edits two crates, and live probes run whether or not a rule needs them. Its interface is wider (a facts schema), and it hides less.
- **A caching probe in sys.** Sys memoizes `lstat` and owner answers per run behind the trait. It shares ancestor work with a smaller core, but the cache is invisible to the guard, so `recheck` would read discovery-era answers unless every sink remembered to invalidate it. That is SAFE-10's failure mode, built in.
- **Typestate per lifecycle stage** (`Discovered`, `Filtered`, `Sized`, `Approved`, from section 3.5). Only the last transition gates a capability. The middle types repeat the path and its invariants at each boundary (temporal decomposition) without preventing any bug `Approved` does not already prevent.
- **Deny-only resolution of linked ancestors**, judging both spellings. It honors SAFE-05's positive control, but it puts two paths in play (the one judged and the one deleted). It also lets a redirected cache root offer Documents' contents, and it still fails the sink's `O_NOFOLLOW` walk.
- **A `Policy::unavailable()` value that refuses everything** (SAFE-07's suggested test). It is equivalent in safety but adds a state every caller can reach. `Result` at build time makes the unavailable guard unrepresentable.
- **`evaluate` only, with a leaf `lstat` each time.** One method fewer. The measurement above shows it misses the 200,000 per second budget under vnode-cache pressure on this machine.

## Implementation reconciliation

The implementation kept the public surface of the sketch: `AbsPath`, `Name`, `escaped`, `FileId`, `Node`, `PathIdentity`, `Probe`, `Guard::{new, pass, recheck}`, `Pass::{evaluate, candidate}`, `Verdict`, `KeptReason`, `Layer`, `Candidate`, `Rule`, `Sink`, `Approved`, `Refused` and `Size`. The three compile-fail doctests are unchanged. Each difference below was accepted during the build.

### Files

The checklist names six core files and one identity file per OS. The shipped layout differs where the sketch did.

| Checklist | Shipped | Why |
|---|---|---|
| `dustpan-core/src/{path,identity,size,candidate,guard,protect}.rs` | The same six, plus `probe.rs`, `probe/fake.rs`, `guard/{pass,verdict,approved}.rs` and `protect/{table,compiled,names,live,macos,macos_bundles,windows,linux}.rs` | The probe trait is the seam to the OS. `compiled.rs` reached 1,246 lines, so name matching and the live-state rules moved to `names.rs` and `live.rs`. `macos_bundles.rs` is generated. |
| `dustpan-sys/src/{macos,windows,linux}/identity.rs` | `unix/identity.rs`, `windows/identity.rs`, `{macos,linux,windows}/process.rs`, `probe.rs` | macOS and Linux share one `lstat`. Only the process tables differ per OS. |
| `dustpan/src/cmd/explain.rs`, `tests/fixtures/mkhome.sh` | The same | |

### Types

- `KeptReason::Identity(IdentityReason)` replaces `ShapeReason::IdentityChanged` (graft 2). `IdentityReason::Changed` means the object differed before the live probes. `ReplacedDuringCheck` means it differed after them. The codes are `identity-changed` and `replaced-during-check`.
- `Proc { pid, exe, args }` replaces `Proc { pid, command }`. A process-name owner matches the file name of `exe`, and a command line with spaces cannot be split back into one.
- `Effect::AllowBundles` is new. Mole lets a container's `Data/Library/Caches` and `Data/tmp` through although the container's own id is protected. A plain allow row cannot say that, because a bundle-id match on a deeper name would win.
- `Table.names` (`bundle_globs`, `bundle_parents`, `keywords`) replaces `Table.bundle_globs`, and `Table.data_volume` replaces `Table.firmlinks`. Keywords are Mole's `*x*` path globs. `bundle_parents` marks `Library/Containers` and `Library/Group Containers`, whose children are named by bundle id.
- `KeptReason::source()` returns `Option<&AbsPath>` and is `None` for every reason until M0-4 (graft 3).
- `Name::as_os_str` and `Unseen::sentence` are new accessors.

### Policy matching

- Carve-out rows are inserted into both tries. There is no third trie.
- One contest decides layers 2 and 4. Every matching rule enters with the depth of the component it matched. The deepest wins. At equal depth a refuse row beats an allow row, and an allow row beats a name pattern. A row anchored on the entry beats a `Children` row on its parent, so `/Users/Shared` is a system folder and not another account's home.
- A bundle glob that starts with `*` and has an inner `*` is matched against the whole path, as Mole does. Mole has one such glob, `*cloudflare*warp*`, and it can match across a separator.
- A `RefuseWithAncestors` row with a glob anchor cannot be probed, so its ancestors refuse without a probe.
- `Guard::new` costs a few hundred `lstat` calls, not about 120. The folders above each anchor are resolved component by component with `probe::physical`.
- An anchor that is itself a link binds the link, not the folder behind it. Windows ships `C:\Documents and Settings` as a junction to `C:\Users`. Bound to its target, that row kept every file in every profile, and CI lane 10 caught it on a real runner.
- A path that does not exist is judged by spelling, so `explain` answers for a path the user has not created yet. Its identity is `None`.
- The pass that judges a leaf link's target takes its own process table.
- The SQLite family names the path as spelled first, because a companion can differ in case. A companion whose name is not UTF-8 cannot be split and is kept as `file-open-unknown`.

### Tables

- macOS is the union of Mole's two lists: the shell validator and analyze's Go list. Clean's `*/Mobile Documents*` rule keeps the whole iCloud tree, which subsumes analyze's three structural depths. So an analyze Trash move (M1) is stricter in dustpan than in Mole: it also keeps names that match a bundle glob, and every file under iCloud Drive.
- `~/.claude` follows Mole exactly: the folder itself, `projects`, `file-history` and `history.jsonl`. The sketch kept the whole subtree, which would block purge of `~/.claude/worktrees`.
- Mole's own log folder becomes dustpan's (`~/Library/Logs/dustpan`). Steam's library is protected in uninstall mode only and arrives with uninstall.
- Three over-protections are accepted and marked in the table: keywords are folded where Mole spells some in one case, `com.fabfilter.*.plist` drops Mole's version-digit limit, and the WeChat web-profile cache carve-out is left out.
- Linux rows come from `linux.md` and Mole's `linux` branch (`lib/core/delete.sh:20-37`, analyze's list, `AGENTS.md:29`). The sketch called every row an inference. The rows with no Mole source are marked inferred: other account roots, `/tmp`, `/opt`, `/srv`, `/mnt`, `/media`, the XDG user folders, the XDG base folders, and `/var/home` for Fedora Atomic.
- Windows keeps a literal `C:` row beside each `%SystemDrive%` row. `AbsPath::parse` upper-cases the drive letter and refuses a `name:stream` component.

### Probes

- The macOS process table holds executable paths only. Arguments need `KERN_PROCARGS2`, which answers for the caller's own processes alone. M3 adds them with the open-file probe (SAFE-08's first milestone).
- A macOS process whose path the kernel will not give is named by its short record (`PROC_PIDT_SHORTBSDINFO`). The table is incomplete only when a listed process has neither. GitHub's macOS runners have such a process, and without this every cache there was `owner-unknown`.
- Linux marks the table incomplete when a `cmdline` cannot be read or when pid 1 is not listed, which is what `hidepid` looks like.
- Windows answers `Unsupported` for the process table. No Windows rule asks for it in M0.
- Windows falls back to the 64-bit file index when a volume does not answer `FileIdInfo` (FAT, exFAT).

### `explain`

- `mode` is `"read-only"` and `run_id` is `<unix seconds>-<pid>`. The keys are written by hand in a fixed order, and `serde_json` escapes each string.
- After a `symlink-ancestor` refusal, text output adds a `physical path:` line. JSON does not carry it.
- A home folder that does not exist is used as spelled. One that cannot be inspected stops the command with exit 1.

### Tests and bench

- `FakeProbe` compiles for core's own unit tests as well as with `testkit`.
- SAFE-27 is tested with a small table inside `protect/compiled.rs`. On the shipped tables every ancestor of Docker and OrbStack state is already a critical folder, so layer 2 answers first and `contains-protected` cannot be seen.
- The bench carries one `expect(clippy::disallowed_methods)`, because it builds and walks a real tree.
- `crates/dustpan-core/tests/guard_windows.rs` holds the section 2.9 cases. They run on the Windows runners only, because `AbsPath` is host syntax.

### SAFE rules this code governs

Each test fails when its guard is removed. The owner ran one mutation per rule marked with a star and saw the named test go red.

| Rule | Test |
|---|---|
| SAFE-01 | `crates/dustpan-core/clippy.toml` repeats the four bans; a temporary `remove_file` in core failed clippy |
| SAFE-02* | `path::tests::malformed_paths_are_refused_with_their_cause`, `a_name_with_two_dots_inside_is_valid`, `explain.rs::a_control_byte_is_refused_and_printed_escaped` |
| SAFE-03* | `path::tests::equivalent_spellings_normalize_to_one`, `guard.rs::a_data_volume_spelling_is_judged_as_its_root_spelling` |
| SAFE-04* | `guard.rs::critical_roots_are_kept_and_their_children_are_judged_on_their_own`, `every_account_root_is_kept_and_the_empty_variable_collapse_is_caught`, `an_alias_that_folding_misses_is_kept_by_file_identity`, `path::tests::join_with_a_name_never_yields_the_parent` |
| SAFE-05* | `guard.rs::a_symlinked_ancestor_is_kept_at_layer_one`, `a_redirected_cache_root_refuses_everything_below_it`, `a_leaf_link_is_judged_by_its_target_and_only_ever_refused`, `an_alias_never_grants_what_its_own_spelling_lacks` |
| SAFE-06* | `guard.rs::a_live_powerlog_store_is_kept_inside_the_allowlisted_tree`, `an_endpoint_security_cache_is_kept_inside_the_allowlisted_tree`, `layers_decide_in_order` |
| SAFE-07* | `guard.rs::an_unreadable_protected_anchor_means_no_guard`, `a_home_that_is_a_system_root_means_no_guard`, the third compile-fail doctest on `Approved`, `explain.rs::a_home_that_is_a_system_root_stops_the_command` |
| SAFE-08 | `probe::tests::only_a_complete_table_proves_an_owner_idle`, `guard.rs::a_running_or_unknown_owner_keeps_its_cache` (the tri-state; the open-file probe is M3) |
| SAFE-09 | `guard.rs::an_open_sqlite_companion_keeps_the_whole_family`, `an_unanswered_open_file_probe_keeps_the_family` (the family rule; the probe is M3) |
| SAFE-10* | `recheck.rs`: `a_swapped_inode_is_refused`, `a_swapped_parent_is_refused`, `a_parent_replaced_by_a_symlink_is_refused`, `a_target_swapped_during_the_owner_probe_is_refused` |
| SAFE-17* | `crates/dustpan/tests/seams.rs` |
| SAFE-18 | `Candidate::new` is `pub(crate)`, and `tests/usage.rs` type-checks the executor that shares `recheck` between preview and apply |
| SAFE-21 | `size::tests::a_partial_size_is_marked_and_an_unknown_size_is_never_zero` |
| SAFE-22 | `path::tests::control_bytes_print_escaped`, `explain.rs::json_keeps_a_hostile_name_inside_its_string` (escaping; the log rows are M0-5) |
| SAFE-27* | `protect::compiled::tests::a_folder_that_contains_protected_state_is_kept_while_the_state_exists` |
| SAFE-35* | `guard.rs::every_reason_has_a_stable_code_a_layer_and_a_sentence`, `explain.rs` exit-code tests |
| SAFE-37 | `guard.rs::system_trees_are_kept_with_everything_below_them` (`/Library/Updates`, `/macOS Install Data`) |
| SAFE-38* | `guard.rs::a_mount_point_is_kept_and_its_contents_are_judged_on_their_own` |

## Open questions and risks

- Does the operator accept the SAFE-05 deviation, refusing every linked ancestor, given that `explain /tmp/x` on macOS prints `Kept · symlinked ancestor /tmp` with the physical path as a hint?
- Should `explain --json` follow PLAN's envelope (`schema_version`, `command`, `items[]`), with lane 8 reading `.items[0]`? Or should it print one bare object, as lane 8's wording suggests?
- Is the scan-shaped number the right gate for the 200,000 per second budget? The `explain`-shaped number depends on the machine's vnode cache and swung tenfold between runs here.
- `recheck` takes one process table per candidate whose rule has owners. Is a few milliseconds per owned candidate at the sink acceptable, or should M3 measure it before choosing a narrower probe? A cache across candidates would reopen SAFE-10.
- Should the binary resolve `HOME` physically before building the guard? On a host where `/home` is a symlink (Fedora Silverblue), every literal home path is otherwise refused as below a linked ancestor.
- The Linux table is inference, since the inventory did not read Mole's Linux lists. Who confirms it before M9?
- Bind mounts on Linux share `st_dev`, so `MountPoint` misses them. Does M0-5's sink need a mount-table check as well?

## Next implementation step

M0-4 adds the whitelist as layer 5: a required `whitelist` field on `GuardInputs`, a `KeptReason::Whitelisted` variant whose `source()` names the file, and `source` filled in `explain --json`.
