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
| `Owner`, `OwnerState`, `ProcessTable`, `OpenFiles` | `crates/dustpan-core/src/probe.rs` | Literal owner names (SAFE-33). `OwnerState::permits_removal` is the one translation of the tri-state (SAFE-08). Owner matching against the table is pure core code. |
| `Policy` (private) | `crates/dustpan-core/src/protect/` | One `Table` of rows per OS, compiled into three tries (critical, protection, carve-outs), an identity table and verified firmlinks. |
| `Guard`, `GuardInputs`, `PolicyError` | `crates/dustpan-core/src/guard.rs` | The only public face of the policy. Built by `Guard::new` or not at all. |
| `Pass`, `Judgment` | `crates/dustpan-core/src/guard/pass.rs` | A stack of ancestor frames shared by every path a pass judges, plus at most one process table and one snapshot of open files. |
| `Verdict`, `KeptReason`, `Layer`, `Hit`, `Kind`, `ShapeReason`, `LiveReason` | `crates/dustpan-core/src/guard/verdict.rs` | Refusals as data. The outer `KeptReason` variant is the layer, so a reason cannot name the wrong layer. Stable codes for `--json` and the oplog. |
| `Rule`, `Sink`, `Candidate` | `crates/dustpan-core/src/candidate.rs` | A rule's shared facts (owners, sink). A candidate carries one discovery verdict and identity, built only by `Pass::candidate`. |
| `Size` | `crates/dustpan-core/src/size.rs` | `Known`, `Partial`, `Unknown`; unknown never renders as 0 (SAFE-21). |
| `Approved`, `Refused`, `Rejection` | `crates/dustpan-core/src/guard/approved.rs` | An approval only `Guard::recheck` builds. Not `Clone`, `Default` or `Deserialize`. |
| `LiveProbe` | `crates/dustpan-sys/src/probe.rs` | The real `Probe`, with `lstat`, `readlink` and a process table. `unix/identity.rs` is real code. Windows identity and the process tables are `todo!()` with the API named. |

### How core reaches the file system without OS calls

Core defines `Probe` with four questions: `node` (one `lstat`), `link_target` (one `readlink`), `processes` (one process-table snapshot) and `open_files` (one snapshot of the files every process holds open; before fix round 3 this was `in_use`, asked per SQLite family). Every decision about what to ask and what an answer means stays in core. Symlink resolution (`probe::physical`), owner matching (`Owner::state_in`), the SQLite family rule, firmlink mapping and alias detection are all pure core code over probe answers. `dustpan-sys` implements the four questions with syscalls and keeps no state, so every answer is fresh.

A core unit test passes `probe::fake::FakeProbe`, an in-memory tree where two spellings that share a `FileId` are aliases. It can model a case alias, a firmlink, a symlinked ancestor, an unreadable directory, a running owner, an unreadable process table, an open WAL file, and a swap between discovery and the sink. It also counts calls, so a test asserts ancestor sharing by what the probe saw, not by reading internals. The fake compiles only under the `testkit` feature, which core's integration tests turn on through a self dev-dependency. That dev-dependency is verified to compile. No shipped crate names `FakeProbe` or `testkit`, and M0-3 adds a source test that fails if one does (SAFE-17).

Core's integration tests and bench can also dev-depend on `dustpan-sys`. They link the same core library sys does, so `LiveProbe` satisfies core's `Probe` there. The bench compiles with `&LiveProbe` as a `&dyn Probe`, which proves the types unify. So M0-5's `crates/dustpan-core/tests/sink.rs` can drive the real sinks. Only `#[cfg(test)]` unit tests inside core would see two copies of core.

### Data flow

1. **Build.** `Guard::new(GuardInputs { os, home, env }, &probe)` selects the OS table and expands `~` and `%VAR%` anchors. It fails with `HomeIsCritical` when `HOME` is `/` or `/Users`. It inserts rows into the tries, and records the `FileId` of every anchored refuse row that exists, about 120 `lstat` calls once per process. An anchor that exists but cannot be inspected fails the build. It keeps each macOS firmlink only after `/<x>` and `/System/Volumes/Data/<x>` prove to be one directory.
2. **Judge.** `Pass::judge(path, seen, owners)` runs four layers in order, and the first refusal decides.
   - Layer 1, shape. The path is mapped to its firmlink spelling. The pass reuses frames for ancestors it already holds and `lstat`s only new ones. Any linked ancestor refuses (`LinkedAncestor`), and so does an unreadable one (`Unreadable`). A leaf whose volume differs from its parent's is a mount point (`MountPoint`, SAFE-38). A leaf link has its target resolved and judged by a nested pass, and that target can refuse but never grant (`LinkToProtected`, SAFE-05).
   - Layer 2, critical. The deepest matching anchor wins, carve-out rows join as allow rows, and refuse beats allow at equal depth. Identity rows only refuse. They compare the leaf with Exact and Subtree anchors, the parent with Children anchors, and every ancestor with Subtree anchors (already folded into its frame).
   - Layer 3, live. PowerLog `Current*` stores and EDR caches refuse by name. Owners come from the rule plus the bundle id of a user-cache scope, and are matched against the pass's single process table. These run even under carve-outs (SAFE-06). Since fix round 3 the layer also asks who holds the path, anything below it or a member of its SQLite family open, from one snapshot per pass, and it asks that last, after layers 4 and 5.
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
- **M3, rules with owners.** The catalog loader builds `Arc<Rule>` and may add fields (roots, minimum age, label). `Owner` may gain variants, and `OwnerCommand` gains variants. Fix round 3 already reads open files on macOS and Linux. Review-only rows become planner rows that never reach `Pass::candidate`.
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
- We accepted `in_use` answering `Unknown` until M3, which kept every SQLite-family and partial-download path as `file-open-unknown`. Fix round 3 ended that on macOS and Linux. On Windows the answer is unknown for every path no rule keeps, until M8.
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

- The macOS process table holds each executable path, the id of every bundle that encloses it and, since fix round 2, the signing identifier and the app groups from the process's code signature. It holds no arguments: they need `KERN_PROCARGS2`, which answers for the caller's own processes alone. M3 adds them with the open-file probe (SAFE-08's first milestone).
- A macOS process whose path the kernel will not give is listed by its short record (`PROC_PIDT_SHORTBSDINFO`) and marked unidentified. The table is incomplete only when a listed process has neither. See "Fix round 1" for what an unidentified process means for owners.
- Linux marks the table incomplete when a `cmdline` cannot be read or when pid 1 is not listed, which is what `hidepid` looks like.
- Windows answers `Unsupported` for the process table. No Windows rule asks for it in M0. Since fix round 3 it answers `Unsupported` for open files too, which every path no rule keeps asks for.
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

### Fix round 1: aliases, volfs and running owners

An adversarial review at `c964f04` broke the guard's intent in three ways. Each fix below names the root cause, the shape chosen and the reason.

**Alias spellings (F3).** `recheck` approved a Kelvin-sign spelling of `Library/Keychains/login.keychain-db` for `Delete`. Layer 4 matched rows by spelling with ASCII folding, and file identity covered anchored rows only. A floating row (`Library/Keychains`), a bundle glob and a keyword have no anchor to bind, so any spelling the volume resolves to the same file passed. Bundle globs were also matched with case, so `COM.MICROSOFT.VSCODE` passed with no Unicode at all.

- `Probe::spelling` is a fifth probe question: the spelling the file system stores for an existing path, last component not followed. macOS answers with one `getattrlist(ATTR_CMN_FULLPATH, FSOPT_NOFOLLOW)`. Windows answers with `GetFinalPathNameByHandleW` on the handle `node` already opens. Linux answers with the path itself.
- `getattrlist` was chosen over `open` plus `fcntl(F_GETPATH)`. It needs no permission on the file, so a mode-000 file and `/private/etc/sudoers` still answer, and it opens no descriptor on a FIFO or a device. Measured on this host, both calls gave the same answer for case, case-fold, normalization, firmlink and volfs spellings. They differ for `/.nofollow/System`: `getattrlist` echoes it back. The `/.nofollow` row below covers that.
- `Pass::judge` judges a path that every layer passed a second time, under its stored spelling, when the two differ. The second judgment can only refuse. The refusal is `ShapeReason::AliasOfKept { stored, layer }`, code `alias-of-kept`. A stored spelling that cannot be read keeps the path as `unreadable`.
- `evaluate` and `recheck` always ask. `candidate` asks only when the path holds a byte outside ASCII. A scanner's names come from a directory read, so they are stored spellings already. An all-ASCII name has one kind of alias on a case-insensitive volume, ASCII case, and folding covers it. A case fold or a normalization form needs a byte outside ASCII. This keeps the scan shape at zero extra syscalls for ASCII names. `recheck` asks for every candidate regardless, so an alias the scan could not tell apart is still refused at the sink.
- Bundle globs are folded like every other name when the table folds case. A bundle id names one app on a case-insensitive volume however it is spelled. This can keep more than Mole does and never less.
- Not covered: a hard link to a protected file under another name. Removing it leaves the protected name and its data in place.

**volfs (F4).** `/.vol/<device>/<inode>` names any file by number. `lstat /.vol/<device>` is `ENOENT`, so the pass recorded the ancestor as missing and judged the leaf by spelling alone.

- A leaf that exists under a folder the pass recorded as missing is kept. If its stored spelling is kept, the reason is `alias-of-kept` and names that spelling. Otherwise it is `ShapeReason::ParentNotFound`, code `parent-not-found`: no ancestor of the path was inspected, so what it names is unknown. `Pass::candidate` hands back a `Judgment` for such a path, so no candidate exists for `recheck` to see.
- `/.vol`, `/.nofollow` and `/.resolve` are critical system trees on macOS. All three reach every file by another route, and none holds anything to clean. The lookup of these names is case-sensitive (`/.VOL/...` is `ENOENT`).

**Running owners (F1).** Ghostty ran and its cache read `Eligible`. The macOS table held executable paths only, and `Owner::matches` looked for the bundle id's words in the path. `/Applications/Ghostty.app/Contents/MacOS/ghostty` does not spell `com.mitchellh.ghostty`. A process whose binary was deleted fell back to a 16-byte short name, which can never spell an id, and the table still counted as complete.

- `Proc` gains `app: App`. `App::Bundles` holds the id of every bundle that encloses the executable, innermost first, so a helper names its own bundle and the app that ships it. `App::Unknown { mine }` is a process the probe could not identify.
- The macOS table reads `CFBundleIdentifier` from each enclosing bundle's `Info.plist`: `X/Contents/Info.plist`, and `X.app/Info.plist` for an iOS app on a Mac. Core Foundation parses the bytes (`CFPropertyListCreateWithData`), so XML and binary lists both work. Each folder is read once per snapshot. No subprocess runs. `core-foundation-sys` is the one new dependency, on macOS only. LaunchServices through `objc2` was not needed.
- A bundle owner matches a process whose bundle ids include the owner's id, compared without ASCII case. The path-word match stays as a second signal. It is the only signal on Linux, and anywhere it can only add a refusal. Fix round 2 limits it to a process in no bundle, because it named unrelated processes as owners.
- A process is unidentified when its executable path is gone, or when an enclosing `Info.plist` exists and cannot be read or parsed. An unidentified process of the current user makes every owner that no process matches `Unknown`, which keeps the path (SAFE-08). The refusal names the pid: `process 78754 cannot be identified; quit it first`. Another user's unidentified process is set aside: it cannot own a cache in this user's home. When dustpan runs as root, every process counts as its own.
- A missing `Info.plist` is not an unreadable one. A folder named `Contents` with no `Info.plist` is not a bundle, and the process in it has no bundle id.
- Cost: a snapshot of 764 processes took 12.6 ms on the bench host (median of seven in one process; the first took 34 to 37 ms), up from 3.2 ms, because about 170 property lists are read and parsed. A pass takes one snapshot. `recheck` takes one per candidate that asks about an owner. M0-5 must decide how a batch of rechecks shares one table without reopening SAFE-10; see the open question below.
- Not covered: a bare executable (no bundle) whose cache folder is named by a bundle id that its path does not spell. An embedded `__info_plist` section is not read. Fix round 2 covers the case where that id is the executable's signing identifier.

**Tests.** The live alias and volfs tests are in `crates/dustpan/tests/aliases.rs`, not in core's `tests/recheck.rs`: core's `clippy.toml` bans OS calls in every core target, and which names are one file is the volume's decision, so the fake cannot stand in. `FakeProbe::alias` now also models an alias below a folder that is missing, as volfs has. `crates/dustpan-sys/tests/process_identity.rs` spawns a copy of the test binary from a fake `.app`, not a copy of `/bin/sleep`: macOS kills a system binary that runs from another place.

### Fix round 2: bounded bundle reads, app groups and layout words

A second adversarial review, on the fix round 1 diff, broke the intent three more ways. Each fix names the root cause, the rule chosen and the evidence for it. The evidence files are in `/tmp/swarm-m0-3/fix2/`.

**Unbounded `Info.plist` read (R2-F1).** The process snapshot read the `Info.plist` of every folder that encloses every process, of any user, with `std::fs::read`. Any local user can put any file next to a process they run. A FIFO there blocked the open forever, so `explain`, a scan and `recheck` never returned. A link to `/dev/zero` grew `explain` by 6 GB in one second.

- The file is opened with `O_NOFOLLOW | O_NONBLOCK`. The descriptor is then checked with `fstat`, and only a regular file of at most 1 MiB is read, through a reader capped one byte above that so a file that grows after the check is caught.
- The cap is 1 MiB because the largest of 5,930 `Info.plist` files on the bench host is 357 KB (`CoreTypes.bundle`'s `MobileDevices.bundle`).
- A link, a FIFO, a device, a larger file or an open that fails is unreadable. That leaves the process unidentified, which keeps every owned cache of the current user (round 1's rule). A missing file is still "not a bundle".
- A real bundle does not link its `Info.plist`: the live table of 794 processes still has no unidentified process.
- Not covered: a regular `Info.plist` on a network mount that has stopped answering. `lstat` on a path of that mount blocks the same way, in every layer.

**Group container caches (R2-F2).** Warp ran, its cache in `Library/Caches` was kept, and the caches in `Library/Group Containers/2BBY89MBSN.dev.warp` read `Eligible`. The owner of a group container scope was taken to be a bundle id, and no process has a group's folder name as its bundle id.

- `Owner::AppGroup` is a new owner, named by the group's folder. The policy table lists group scopes apart from bundle scopes (`group_cache_scopes`). A rule can name a group as an owner too.
- Rule 1, entitlement. A process owns a group when its entitlements name the folder under `com.apple.security.application-groups`. This is the list the system checks before it lets a sandboxed process into the folder, so it is exact. The macOS table reads each process's entitlements from the kernel with `csops(CS_OPS_ENTITLEMENTS_BLOB)`. No subprocess runs and no file is read, and the call answers for every user's processes without privilege. Most processes name no group, so their list is searched for the key and never parsed.
- Rule 2, name. An app outside the sandbox needs no entitlement to use a group folder. So a group also matches the app its folder name holds. The candidates are the whole name, the name after a ten-character team id, and that name after `group.` or `groups.`, each only when it has a dot. A process matches a candidate when one of its ids equals it, or when one extends the other at a dot (`dev.warp.Warp-Stable` and `dev.warp`). A process in no bundle also matches when its command line spells the candidate, as for a bundle id.
- A process's ids are the ids of the bundles around its executable and, new in this round, its signing identifier (`csops(CS_OPS_IDENTITY)`). The signing identifier is the only id a daemon in no bundle has. It also applies to bundle owners, so `com.apple.bird` now keeps `Library/Caches/com.apple.bird`.
- Rejected, a team id rule: a group `<TEAMID>.<name>` matches any running process signed by that team. It errs further toward keeping, and it is not true. Measured on the bench host over 201 group folders: 84 have a running process that is entitled to them, 10 more are matched by name alone (`com.apple.bird`, `group.com.apple.chronod`, `group.com.apple.photolibraryd.private`, all true owners outside the sandbox), and 14 would be matched by team alone. Those 14 tie Visual Studio Code to twelve Microsoft groups, `adb` to Google Drive's group, and Tailscale's standalone build to the App Store build's group. `explain` would name pids that do not use the folder, which is the defect R2-F3 removes.
- Before and after on the real home: of 1,409 cache scope folders, 80 group cache folders went from `Eligible` to kept as running. One folder went the other way, `Library/Containers/net.blip.macos.share`: it had read as running through the words `macos` and `share` in another app's path.
- A signature the kernel will not give, or entitlements that name groups and cannot be parsed, leave the process unidentified. No process on the bench host is in that state.
- Cost: one snapshot of 793 processes takes 35 ms (median of seven), up from 13 ms, for two `csops` calls per process.
- Not covered: entitlements stored in DER form only. All 735 running processes that have entitlements carry the XML form too. An unsandboxed, unentitled process whose ids are unrelated to the folder name is also not covered: nothing ties it to the group.

**Layout words as evidence (R2-F3).** `explain` said `com.raycast.macos.accessibility is running` and named the pids of `mediaaccessibilityd`. Raycast was not running. The path-word match ran for every process, and it asks for the id's last segment and one earlier segment as whole words in the command line. Every bundled executable path holds `/Contents/MacOS/`, so `macos` corroborated any id that has it.

- A process in a bundle matches by id and nothing else. The bundle's `Info.plist` says which app it is, so its path adds no evidence and can only mislead.
- A process in no bundle keeps the word match. On Linux it is the only signal, and on macOS it still covers a bare executable. In it, the words of the system layout and the bundle layout no longer count, for the last segment or the corroborating one. The whole id spelled in the command line still counts.
- The list is `LAYOUT_WORDS` in `crates/dustpan-core/src/probe.rs`. The macOS words are derived, not guessed: every word of four letters or more in 1,975 executable paths (running, or installed under `/Applications`, `/System`, `/usr` and `/opt/homebrew`) was counted by the number of unrelated products whose paths hold it. The words held by 12 or more products are `system` 467, `contents` 441, `macos` 441, `library` 415, `libexec` 409, `coreservices` 202, `sbin` 197, `framework` 195, `applications` 159, `privateframeworks` 154, `versions` 95, `bundle` 86, `xpcservices` 82, `support` 73, `plugins` 60, `frameworks` 59, `appex` 56, `apple` 48, `helper` 37, `resources` 32, `plugin` 21, `utilities` 21, `renderer` 18, `helpers` 13 and `application` 12, plus three plug-in folder names of single subsystems. The list takes the ones that name a folder. It leaves `apple`, which is a vendor, and `helper`, `plugin` and `renderer`, which are parts of product names. It adds the bundle folders Apple documents that are rare on this host (`loginitems`, `sharedsupport`, `current`), the `hier(7)` names `private`, `users`, `volumes`, `local` and `share`, and the Linux folders that hold executables under the FHS, snap and flatpak (`home`, `games`, `lib32`, `lib64`, `libx32`, `snap`, `flatpak`).
- Words under four letters (`usr`, `bin`, `var`, `opt`) never counted, so they are not in the list.
- The Raycast cache now reads `Kept · protected app data`, which is the true reason: Raycast is a protected bundle. `com.example.macos.helper` reads `Eligible`.
- Not covered: two products that share two product words. `com.example.editor.helper` still matches a bare `/opt/example/bin/helper` of another vendor named example.

**Tests.** `crates/dustpan/tests/hostile_plist.rs` and `owners.rs` run a copy of the test binary from a folder they lay out and judge with the real `explain` under a one-second limit. The hostile cases run one at a time, because each leaves an unidentified process behind while it runs. `owners.rs` signs one copy ad hoc with an app group entitlement, which the system accepts. The first run of a new binary made the next process snapshot wait 0.5 to 0.9 seconds on the bench host, so each fixture runs one untimed `explain` before the timed ones.

### Fix round 3: open handles

A third adversarial review, on the fix round 2 diff, broke "a running app's data is kept" again (R3-A1) and made the guard slow (R3-A2). Before a third fix, the premise under the first two was written down and tested. The evidence files are in `/tmp/swarm-m0-3/fix3/`.

**The premise.** Every earlier fix answered "is this path's owner running?" by naming the owner and matching processes to the name: words of the executable path, then bundle ids, then signing identifiers and app groups. All of them assumed one thing: the guard can name the owner of a path.

**The test that falsified it.** If the premise held, no cache folder in which a process holds a file open would read `Eligible`. `scripts/open-handles-check.sh` lists every `~/Library/Caches/<name>` and `Group Containers/<name>/Library/Caches` folder in which `lsof` shows an open file of this user's processes, and asks `explain` about each. At `a2b9f07`, 7 of 41 read `Eligible`: `BraveSoftware` while Brave ran, `CloudKit`, `GeoServices`, `copilot`, `docker-secrets-engine`, `familycircled` and `group.com.apple.storekit/Library/Caches`, where `storekitagent` held `storeUser.db` and its WAL. Each is named for a vendor or a daemon. No rule about names reaches them, so a fourth rule about names would have failed the same way.

**The new shape.** The guard now reads the direct signal PLAN's layer 3 already names: no handle is open.

- `Probe::open_files` replaces `Probe::in_use`. It returns one snapshot, `OpenFiles`: a `(pid, path)` for every file, folder and bound Unix socket a process holds, and the processes whose list could not be read (`Unlisted { pid, mine }`). The `Handles` type is gone.
- `guard::open::OpenIndex` sorts the snapshot once per pass. "Is anything open at this path or below it" is two binary searches, whatever the folder holds. Keys are lowercased where the policy folds case.
- A path is kept with `LiveReason::InUse { pid, file }` when a process holds open the path, anything below it, or a member of its SQLite family. The code stays `file-open` and the layer stays `live`. The sentence is `process 13534 has <file> open; quit it first`.
- A path that nothing holds is still kept, as `file-open-unknown`, when the snapshot failed or a process of this user did not list its files. The refusal names that process. Another user's unlisted process is set aside, as an unidentified one is in round 1: a run that is not root can list no root process, so it could otherwise call no path free.
- The unknown applies to every path, not only to paths under the home folder. A process of this user can hold a file in `/private/var/folders` as well. On the bench host no process of this user is unlisted (0 of 541), so the wider rule costs nothing there.
- The question is asked last, for a path every other rule lets go. Its answer can only keep a path, so no allowlist skips it (SAFE-06). A protected path keeps its own reason: `explain ~/Library/Keychains/login.keychain-db` says `keychains`, which stays true after the app quits, and a pass over protected paths takes no snapshot at all.
- `recheck` starts its own pass, so it takes its own snapshot, after the first identity check and before the second (SAFE-10). A file opened between the scan and the sink refuses the candidate.
- Name matching stays as a second signal that can only refuse. An app that runs and holds nothing open in its cache folder at that moment is still kept.
- The snapshot leaves out dustpan's own process. The folders a scan has open are not another program's live data.

**macOS.** `crates/dustpan-sys/src/macos/open.rs` asks the kernel through libproc: `proc_listallpids`, `proc_pidinfo(PROC_PIDLISTFDS)` for each process, and `proc_pidfdinfo` for each descriptor that is a file, a folder or a socket. No subprocess runs.

- The kernel refuses the path of a file in a data vault (`EPERM`), 59 of 5,493 file descriptors on the bench host, all of eight Apple daemons such as `routined`. Those are skipped. Dustpan cannot open or remove such a file either. That the 59 sit in data vaults is inferred from `routined`, whose mapped files are in a `dv` folder; it was not checked for each one.
- A bound Unix socket counts. The first run of the check after this change still read `docker-secrets-engine` as `Eligible`: Docker holds `engine.sock` there as a socket, not as a file. Removing it cuts the daemon off from its clients. The kernel keeps the address as the process spelled it, so its folder is resolved to a physical path (`$TMPDIR` is below the link `/var`). A relative address is skipped, 1 of 110 on the bench host.

**Linux.** `crates/dustpan-sys/src/linux/open.rs` reads the links in `/proc/<pid>/fd`, and `/proc/net/unix` for the path a socket is bound to.

- "This user's process" is one whose descriptor folder, `/proc/<pid>/fd`, this user owns. The kernel gives that folder to root when a process turns off inspection of itself, as `ssh-agent` does, and such a process is set aside like another user's. `/proc/<pid>` itself always belongs to the user the process runs as. The first push judged by it, and the `ubuntu-latest` runner failed three `explain` tests with `the open files of process 1179 cannot be listed`: one such process kept every path no rule protects. A container with `ssh-agent` running reproduced it, and `open_files.rs::a_process_that_turned_off_inspection_is_set_aside` holds the rule. What such a process holds open is not seen; M9 decides whether Linux needs more.
- A process that is starting a new program is shown to root alone for a moment, and its descriptor folder is root's for that moment. In a container, 3 of 9 `explain` tests failed with `the open files of process 2378 cannot be listed`, because a sibling test's `dustpan` was starting. Such a process is now set aside for that snapshot. A refusal from a folder this user does own is asked again, up to four times, a millisecond apart. The same tests then passed 30 runs of 30, and 20 of 20 with `ssh-agent` running.

**Windows.** `open_files` answers `Unsupported` until M8, which adds `NtQuerySystemInformation`. A path that no rule keeps is therefore kept on Windows, as `file-open-unknown`, with the sentence `this build cannot check it yet`. Critical and protected paths keep their own reasons. CI lane 10 and three `explain` tests expect this on Windows.

**Not covered.**

- A file a process has mapped and then closed, and a process's working folder. Neither is a descriptor.
- A file held only by another user's process when dustpan is not root, for example a root daemon that writes into a user's cache.
- An open file the kernel reports under its data-volume spelling (`/System/Volumes/Data/Users/...`). None of 5,439 paths on the bench host has one.
- A file a process opened through a hard link under another name. The kernel reports one path per open file.

**Cost.** One snapshot took 11 to 13 ms on the bench host: 836 processes, 541 of them this user's and listed, 6,206 paths (median of seven in one process, three runs). The pass asks it once per folder: a folder with nothing open in it answers for its files, and for a SQLite family, whose members share the folder. It is linear in open descriptors, about 1.6 µs each: while a Docker VM held 102,000 files of a shared folder open, the snapshot took 181 ms for 114,977 paths. No kernel limit holds the number of descriptors down, because `dup` and `fork` add descriptors that share one open file. A snapshot therefore stops at 262,144 descriptors and answers `TimedOut`, which keeps every path no rule protects. That is about 0.4 s of calls here, by arithmetic, not by measurement. The comment audit found the first version of this claim wrong: it named `kern.maxfiles` as the bound. `docs/perf.md` has the bench. `recheck` pays one snapshot per candidate; see the open question below.

**Capped bundle reads (R3-A2).** One process below 211 folders named like bundles, each with a well-formed 1 MiB `Info.plist`, made every process snapshot take 6.3 s, and five such processes 31 s. Round 2 capped one file and not their number.

- At most 16 folders above one executable are asked for an `Info.plist`. The deepest of 787 processes on the bench host has 6 (a Brave helper inside a framework inside the app). An executable below more is not identified, and nothing is read for it.
- One snapshot reads at most 8 MiB of `Info.plist` files and spends at most 250 ms of CPU on them. The bench host's processes need 242 files, 0.77 MB in all. CPU time is the thread's, so a cold disk does not spend the budget. Past either cap a bundle is unreadable, which leaves its process unidentified (round 1's rule: every owner is unknown and kept).
- With the fixture running, `explain` answers in 0.06 s with one process and with five (`slow_snapshot.sh`).
- Not covered: one user can still spend the shared budget and leave another user's processes unidentified. That keeps more and removes nothing.

**Tests.** `guard.rs` has the rules against the fake: a vendor-named folder kept at every depth with the pid and file named, an unlisted process of this user and of another, a SQLite family that is free once closed, one snapshot per pass and none for a protected path. `recheck.rs` opens a file after discovery. `crates/dustpan-sys/tests/open_files.rs` holds a file, a folder and a socket open in a child process and reads the real snapshot, on macOS and Linux. `crates/dustpan/tests/owners.rs` runs `explain` against a child that holds a file in `Library/Caches/BraveSoftware`, then closes it and stays alive. `hostile_plist.rs` runs a process below the deep tree. Both live tests and the check script failed at `a2b9f07`.

### Fix round 4: a held file the snapshot cannot place

A fourth adversarial review, on the fix round 3 diff, reached `Approved` twice for a folder in which a live process held something. Both times the snapshot of open files took a path on trust that it could not confirm. The evidence files are in `/tmp/swarm-m0-3/fix4/`.

**R4-F1. A path longer than the kernel reports.** `proc_pidfdinfo(PROC_PIDFDVNODEPATHINFO)` answers in a buffer of `MAXPATHLEN`, 1,024 bytes. For a longer path the kernel returns no error. It reports the end of the path, and that end starts with `/` like a whole path.

- A file held six 200-byte folders below `Library/Caches/vendor` was listed at `/dddd…/held.db`. The folder read `Eligible`, and `recheck` approved it with the holder alive.
- A crafted chain made the reported end spell a file in another folder. That idle folder read `Kept · process N has <file> open`, and the folder that was held read `Eligible`.

**R4-F2. A socket address that does not resolve.** A socket bound to `/.vol/<device>/<inode>/s.sock` is in the folder with that inode. `realpath` answers `ENOENT` for the volfs folder, and round 3 then kept the address as spelled, so it matched no folder.

**The rule.** A descriptor is at a path, nowhere, or unconfirmed (`Handle` in `crates/dustpan-sys/src/probe.rs`). An unconfirmed one reports its process in `OpenFiles::unlisted` as a process of this user. That is round 3's unknown: every path no rule protects is kept, and the refusal names the process. The snapshot does not try to rebuild the true path. `OpenFiles::files` still holds what else the process has open, so a folder it holds at a short path keeps the better answer.

- **A reported path is confirmed when it leads to the open file.** The kernel's answer also holds the device and inode of the file the descriptor is open on (`vip_vi.vi_stat`). One `lstat` of the reported path must give the same two numbers. A mismatch, a missing path and a failed `lstat` are all unconfirmed.
- **Only a path of 768 bytes or more is asked.** The kernel builds a path from the file's name upward and stops at the first name that does not fit, with its separator. A name is 255 bytes at most (`NAME_MAX`), so what it reports for a path that was cut is at least 1,024 − 255 − 1 = 768 bytes. 400 files below chains of random name lengths, each at a path of 1,024 bytes or more, all reported 768 bytes or more (`trunc.py`).
- **A shorter path is taken as reported**, as in round 3. Checking every path would not be the same rule: on the bench host 51 of 6,267 reported paths do not lead to their open file (41 are gone, 5 name another file and 5 refuse `lstat`), and each would keep every path. None of the 51 is 768 bytes or longer (`measure.py`).
- **A socket address is confirmed when its folder resolves** to a physical path. A folder that does not resolve is unconfirmed, on macOS and on Linux. On the bench host all 119 absolute addresses resolve.

**Cost.** Nothing measurable. The bench host has no reported path of 768 bytes or more among 6,267, so a snapshot makes no extra call. `scan` measured 660,862 evaluations per second before and 664,166 after, medians of five alternated runs, and the ranges overlap. `docs/perf.md` has the runs.

**Not covered.**

- A relative socket address is still left out, as in round 3. `run.sh relsock` reads `Eligible` before and after. Docker Desktop holds one on the bench host (`docker-desktop-build.sock`), 1 of 120. Counting it as unconfirmed would keep every path no rule protects on any host where Docker runs.
- A file opened through a hard link under another name still reads as held at the one path the kernel reports (`run.sh hardlink`). The same holds past 768 bytes: if the reported end of a long path names a hard link to the held file, it is confirmed.
- A path of 768 bytes or more whose file was unlinked or replaced while open, or that sits where `lstat` is refused, makes its process unlisted. That keeps more and removes nothing.
- Linux reports a descriptor's path through a link in `/proc/<pid>/fd`, and a link that cannot be read is skipped. Whether a path longer than a page reads that way was not run. M9 decides.

**Tests.** `crates/dustpan-sys/tests/unconfirmed_handles.rs` runs the real probe and the real guard against a child process, on macOS. Three tests failed at `f3c8ce7`: a file held past 1,024 bytes, the crafted chain, and the volfs socket. Each asserts that the folder is kept and that `recheck` refuses a candidate found before the child started. A fourth holds the bound from the other side: a file at a 900-byte path is listed by its path under its holder. Unit tests in `macos/open.rs` and `probe.rs` hold the rule for a path that matches, mismatches or is missing on each side of 768 bytes, and for a socket address that resolves, does not, or is relative. `recheck.rs` holds the refusal for an unlisted process at the sink against the fake.

### SAFE rules this code governs

Each test fails when its guard is removed. The owner ran one mutation per rule marked with a star and saw the named test go red.

| Rule | Test |
|---|---|
| SAFE-01 | `crates/dustpan-core/clippy.toml` repeats the four bans; a temporary `remove_file` in core failed clippy |
| SAFE-02* | `path::tests::malformed_paths_are_refused_with_their_cause`, `a_name_with_two_dots_inside_is_valid`, `explain.rs::a_control_byte_is_refused_and_printed_escaped` |
| SAFE-03* | `path::tests::equivalent_spellings_normalize_to_one`, `guard.rs::a_data_volume_spelling_is_judged_as_its_root_spelling`, `an_alias_of_a_path_kept_by_name_is_kept_under_its_stored_spelling`, `a_scanned_name_outside_ascii_is_judged_under_its_stored_spelling`, `a_path_under_a_folder_that_is_not_there_is_kept`, `recheck.rs::an_alias_the_scan_could_not_tell_apart_is_refused_at_the_sink`, `crates/dustpan/tests/aliases.rs` (real volume, macOS) |
| SAFE-04* | `guard.rs::critical_roots_are_kept_and_their_children_are_judged_on_their_own`, `every_account_root_is_kept_and_the_empty_variable_collapse_is_caught`, `an_alias_that_folding_misses_is_kept_by_file_identity`, `path::tests::join_with_a_name_never_yields_the_parent` |
| SAFE-05* | `guard.rs::a_symlinked_ancestor_is_kept_at_layer_one`, `a_redirected_cache_root_refuses_everything_below_it`, `a_leaf_link_is_judged_by_its_target_and_only_ever_refused`, `an_alias_never_grants_what_its_own_spelling_lacks` |
| SAFE-06* | `guard.rs::a_live_powerlog_store_is_kept_inside_the_allowlisted_tree`, `an_endpoint_security_cache_is_kept_inside_the_allowlisted_tree`, `layers_decide_in_order` |
| SAFE-07* | `guard.rs::an_unreadable_protected_anchor_means_no_guard`, `a_home_that_is_a_system_root_means_no_guard`, the third compile-fail doctest on `Approved`, `explain.rs::a_home_that_is_a_system_root_stops_the_command` |
| SAFE-08 | `probe::tests::only_a_complete_table_proves_an_owner_idle`, `a_bundle_id_matches_the_bundle_a_process_runs_in`, `an_unidentified_process_of_this_user_leaves_every_owner_unknown`, `guard.rs::a_running_or_unknown_owner_keeps_its_cache`, `an_app_keeps_its_cache_when_only_its_bundle_names_the_id`, `an_unidentified_process_of_this_user_keeps_every_owned_cache`, `a_group_containers_caches_are_kept_while_an_app_of_the_group_runs`, `a_process_keeps_the_caches_of_the_groups_its_entitlements_name`, `the_words_of_a_bundle_layout_name_no_owner`, `probe::tests::a_layout_word_is_no_evidence`, `an_app_group_matches_its_entitled_processes_and_the_app_it_names`, `crates/dustpan-sys/tests/process_identity.rs`, `crates/dustpan/tests/owners.rs` and `hostile_plist.rs` (real processes, macOS) (the tri-state; the open-file probe is M3) |
| SAFE-09 | `guard.rs::an_open_sqlite_companion_keeps_the_whole_family`, `a_sqlite_family_is_free_once_every_member_is_closed`, `an_unanswered_open_file_probe_keeps_what_no_rule_protects`, `an_open_file_keeps_every_folder_above_it_whatever_the_folder_is_named`, `a_process_that_does_not_list_its_open_files_keeps_what_no_rule_protects`, `a_pass_takes_one_snapshot_of_open_files_and_none_for_a_protected_path`, `guard::open::tests`, `crates/dustpan-sys/tests/open_files.rs` (real snapshot, macOS and Linux), `owners.rs::a_process_keeps_the_folder_it_holds_a_file_open_in` (real `explain`, macOS), `scripts/open-handles-check.sh` (the live machine) |
| SAFE-10* | `recheck.rs`: `a_swapped_inode_is_refused`, `a_swapped_parent_is_refused`, `a_parent_replaced_by_a_symlink_is_refused`, `a_target_swapped_during_the_owner_probe_is_refused`, `a_file_opened_after_discovery_is_refused_at_the_sink` |
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
- `recheck` takes one process table per candidate that asks about an owner. Since fix round 2 a macOS table costs about 35 ms on 793 processes (13 ms after round 1, 3 ms before it), because it reads each running bundle's `Info.plist` and asks the kernel for each process's code signature. That is 35 seconds per thousand owned candidates. M0-5 must choose: one table per recheck batch with the identity check still per candidate, or a table that reads only the bundles the batch's owners could match. A cache across batches would reopen SAFE-10. Since fix round 3 every recheck also takes one snapshot of open files, 11 to 13 ms on the same host and linear in open descriptors, and the same choice applies to it.
- Resolved by issue #5: every `~` row, every `~` whitelist entry, and the live layer's home-relative scopes anchor at both `HOME` and the account's home from the user database (`GuardInputs::homes`), so `HOME=<fixture>` no longer makes the real home's `Documents` eligible. Either home being a critical root stops the build with `HomeIsCritical`.
- Should the binary resolve `HOME` physically before building the guard? On a host where `/home` is a symlink (Fedora Silverblue), every literal home path is otherwise refused as below a linked ancestor.
- The Linux table is inference, since the inventory did not read Mole's Linux lists. Who confirms it before M9?
- Bind mounts on Linux share `st_dev`, so `MountPoint` misses them. Does M0-5's sink need a mount-table check as well?

## Next implementation step

M0-4 adds the whitelist as layer 5: a required `whitelist` field on `GuardInputs`, a `KeptReason::Whitelisted` variant whose `source()` names the file, and `source` filled in `explain --json`.

## M0-4 whitelist

Layer 5 keeps a path the user whitelisted, everything inside it, and every folder above it. M0-5's sinks trust it, so a whitelisted path that reads `Eligible` is a path they would delete. This section records how the layer reads the file, which spellings of an entry it keeps, and how it compares names.

### Reading the file

`dustpan_sys::read_config` judges the file the way Mole's `mole_whitelist_file_state` does (`lib/core/base.sh:418`). It walks the path from the top with `lstat` and follows each link.

- The file is missing only when a component does not exist. Then the default entries apply.
- A link whose target does not exist, a folder on the path that is not a folder, or a file that is not a regular file stops the run. The message names the component. A broken link never authorizes the defaults.
- The read opens with `O_NONBLOCK`, so a FIFO swapped in after the walk cannot block the run. `fstat` must report a regular file.
- At most 1 MiB is read. A hand-kept list of 10,000 entries uses under half of that. A link to `/dev/zero` or a log that keeps growing stops at the cap.

A config path the guard cannot take, such as an `XDG_CONFIG_HOME` with `..`, stops the run with the path and the cause. Whitelist lines with `..` stop the load too, as Mole refuses them.

### Parsing an entry

A leading `~`, `$HOME` or `${HOME}` ends at a separator or at the end of the line, and every separator after it is dropped. Such an entry expands once at each home in `Homes`, so `HOME` pointed elsewhere still keeps the account's copy. `~//x` is the home's `x`, never `/x`. `$HOMEX/y` is not expanded, so it is a relative path and stops the load. On Windows, `~/D:\work` would name another drive, so it stops the load too.

Mole matches an entry with `[[ $path == $pattern ]]`, and its commands export `LC_ALL=C` (`bin/clean.sh:15`). Its `?` and `[...]` therefore take one byte, and its classes hold ASCII only. A name of a dustpan entry matches when any of these holds:

1. Its bytes match the pattern under those C-locale rules, as Mole's would. `caf??` keeps an NFC `café`, whose `é` is two bytes.
2. Its fold key equals the entry's text folded, as Mole's quoted comparison does. An entry for a folder named `[2020] Album` keeps that folder.
3. Its fold key matches the pattern with `?` and `[...]` taking one character and the combining marks after it. `caf?` keeps `café` in either normalization form, and `Stra?e` keeps `STRAßE`.

The first is Mole's meaning. The other two only add names. In every form, `*` is any run inside one name: Mole's `*` also crosses `/`, and an earlier decision row keeps it inside one name. Sets take ranges, `!` or `^` negation, the POSIX classes, and `[=e=]` or `[.e.]` for one character, as the C locale reads them. `\` takes the next byte as written. A `[` with no closing `]` is a plain character, as in a shell.

An unknown class, a range that runs backwards, and `[=ab=]` or `[.ab.]` stop the load and name the line. The C locale has no element of more than one character, so dustpan cannot say what Mole would match.

### Every spelling of an entry

`Guard::new` resolves each entry once through the probe, the way layer 4 binds its anchors.

1. The leading names that hold no wildcard are walked from the root with `lstat`. A link is followed with `probe::physical`, the last name included. Everything inside a link to a folder lies in that folder, so the entry keeps the target too.
2. The walk stops at the first name that does not exist. The physical path found so far is spelled as stored (`Probe::spelling`). The probe resolves aliases folding cannot meet, such as Windows 8.3 short names, so a spelling it cannot give stops the guard build like an unreadable folder.
3. The entry goes into the trie twice, under the data-volume firmlink mapping (`Policy::canonical`). One copy is as written. The other is the found path, with the rest of the entry appended as written. The found names come from disk, so they go in as plain names even when they hold `*` or `[`.

Both copies carry the same entry index, so a hit names the entry as the user wrote it. Every folder above either copy is kept as `contains-whitelisted`. This covers `/tmp/x` judged as `/private/tmp/x`, Fedora Atomic's `/home` link to `/var/home`, and an entry spelled `/System/Volumes/Data/...`.

An `lstat` that cannot answer (`EACCES`, `EIO`) stops the guard build with `PolicyError::AnchorUnreadable` naming the entry, as a protected anchor does. Keeping only the written spelling would protect less than the user wrote if the unreadable name were a link.

Layer 5 binds no file identity. A Linux bind mount of a whitelisted folder at another path is not kept by this layer. Layer 1 refuses every path below a linked ancestor, so a scanner never judges a spelling through a link.

### Comparing names

On a volume that folds case (the macOS and Windows tables), both an entry's names and a judged path's names are compared under one key:

1. Decompose to NFD.
2. Map each character to its uppercase and then to that letter's lowercase, only where each mapping gives one character.
3. Compose to NFC.

All-ASCII names take an ASCII-lowercase fast path, which gives the same key.

The key has to meet every name the volume treats as the same. Matching more can only keep more.

- APFS ignores both normalization form and case. NFD before the case map and NFC after it put canonically equivalent names on one key, as Unicode's canonical caseless match does.
- NTFS folds case through its upcase table, one character for one, and keeps normalization forms apart. The key folds the same case pairs. It also joins normalization forms, which only keeps more.
- The upper-then-lower map joins pairs that lowercase alone misses, such as final sigma and `σ`.
- One-for-one maps keep `?` counting `ß` as one character, so `Stra?e` still matches `Straße`. Full case folding would turn `ß` into `ss`, and `?` would then match less.
- NFC rather than NFD keeps `é` one character for `?` and `[...]`. Both also take any combining marks that follow the character they match.

The resolution through `Probe::spelling` already meets any spelling the volume itself accepts for an existing name. The fold matters for names after a wildcard and for names that do not exist yet.

In the key form, a set compares both cases of the folded character, because the folded name no longer says which case Mole would have seen. A negated set therefore also matches a letter it names in another case: `com.[!z]oo.bar` keeps `com.zoo.bar` on macOS. That keeps more than Mole, never less.

On Linux the key is the name's bytes. ext4 compares bytes, so an NFC entry does not keep an NFD folder there, and the two are different files.

`unicode-normalization` supplies the NFD and NFC tables. It is the one dependency of `dustpan-core`, and it does no I/O.

### Cost

A `Pass` keeps one trie position per ancestor frame, as before. Resolution costs one `lstat` per distinct folder named by the entries' literal heads, plus one stored-spelling question per distinct deepest folder, all at `Guard::new`. `explain` with the bench's 500-line whitelist measured 10.6 ms at this head and 7.5 ms at 13d50e0, as means of 100 alternated runs each. `docs/perf.md` records the scan bench.

### Changes a restack of M0-3 meets

- `Policy::from_table` builds the whitelist after `verified_firmlinks`, because resolution needs `canonical`. The struct starts with `Whitelisted::empty`.
- `Whitelisted::compile` takes the probe and `canonical` and returns `Result<_, PolicyError>`. `Guard::new` can now fail with `AnchorUnreadable` for a whitelist entry.
- `whitelist::LoadError::BadEntry` carries `EntryError` (`Shape`, `Glob` or `OutsideHome`) in place of `ShapeError`.
- `protect/names.rs`, `guard/pass.rs` and `guard/verdict.rs` are unchanged by the fix round.
- The restack onto M0-3's fix round 3 runs layer 5 after layer 4 and before layer 3's open-file question. A whitelisted path keeps the whitelist reason, which still holds after the app quits, and takes no snapshot of open files.
