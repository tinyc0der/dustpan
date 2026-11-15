# S2: macOS Trash method

Spike S2 asks which of `/usr/bin/trash`, `NSFileManager trashItemAtURL` through `objc2`, `renameatx_np` into `~/.Trash`, and Finder AppleScript works over SSH, inside `~/Library/Containers`, and with Put Back. The answer picks the macOS `Trash` sink for M0-5.

- Branch: `spike/s2-trash` at `ed75e603f4c206b84bd90d1d378de24f0c01515b`.
- Program: `spikes/s2/src/main.rs` (`s2trash`, objc2 0.6, objc2-foundation 0.3, libc).
- Scripts: `spikes/s2/run.sh` (the suite), `container-root.sh`, `tcc-probe.sh`, and `putback.py` (operator step).
- Outputs: `spikes/s2/results/run1/`, `run2/` and `container-root/`.
- Machine: macOS 26.6.2 (25G83), an Aqua session, no SSH (`machine.tsv`).

## What the suite does

Each run moves five kinds of item to the Trash with each of the four methods, five times. The items are a 4 KiB file, a folder with three files, a fake `Fake.app` bundle, a file inside `~/Library/Containers/com.dustpan.spike.s2/` (a folder the spike creates), and a second file with the name of one already in the Trash. That is 100 moves per run. For each move the suite records success, the call latency, whether the source is gone, and the name the item has in `~/.Trash`. Finder then lists the Trash items with the run's prefix, and the suite moves only those items out of the Trash and deletes them.

## Results

Every move succeeded in both runs. In all 200 moves the source was gone and the item was in `~/.Trash`. Finder listed 120 items per run, which is the 100 moves plus the 20 items that the collision cases put there first. `cleanup.tsv` shows `left_in_trash 0` after both runs.

| Method | Median latency, run 1 | Median latency, run 2 | Reports where the item landed | Name on collision |
|---|---|---|---|---|
| `/usr/bin/trash -v` | 27.90 ms | 23.50 ms | Yes, in its `Moved "…" to "…"` line | `name.txt 21-52-06-658.txt` |
| `NSFileManager trashItemAtURL` | 1.20 ms | 1.00 ms | Yes, `resultingItemURL` | `name.txt 21-52-06-676.txt` |
| `renameatx_np(RENAME_EXCL)` | 0.10 ms | 0.10 ms | Caller picks the name | caller's rule (the spike used `name 2.txt`) |
| Finder `delete` through `osascript` | 226.70 ms | 233.60 ms | Yes, the returned alias | `name 21.52.08.txt` |

The latencies come from `summary.tsv` in each run. Finder is about 200 times slower than `NSFileManager`, because every call is an Apple event to Finder plus an `osascript` process. The `/usr/bin/trash` medians differ by 16 percent between runs, and the `NSFileManager` medians by 17 percent. Both differences are under 5 ms, and the S1 scans ran on the same machine at the same time. The ranking does not change.

`/usr/bin/trash` fails with the same `NSCocoaErrorDomain Code=513` error as `NSFileManager` on a protected folder (`container-root/run1.tsv`). The inference is that it wraps `NSFileManager`. It exists only on macOS 15 and later (`man trash`).

## Containers

An item inside a container folder moves to the Trash with all four methods.

A container folder itself does not. Shortly after the spike created `~/Library/Containers/com.dustpan.spike.s2`, `containermanagerd` adopted it. It added `com.apple.containermanager.*` extended attributes and a metadata plist. After that, every method failed on the folder:

- `renameatx_np` returned `Operation not permitted`.
- `NSFileManager` and `/usr/bin/trash` returned "couldn't be moved to the trash because you don't have permission to access it" (`afpAccessDenied`).
- Finder showed a modal alert, "The operation can't be completed because you don't have permission to access some of the items" (`container-root/finder-alert.png`). The `osascript` call waited until someone clicked OK.
- `rmdir` also failed (`run1/container-rmdir.txt`).

`container-root.sh` repeats the first three methods and records the same failures twice (`container-root/run1.tsv`, `run2.tsv`). This session has no Full Disk Access or App Management permission. Whether either permission lets the move succeed is untested. The folder is still on disk, and the operator removes it (see the pending steps).

## Privacy protection on the Trash

`tcc-probe.sh` records what this process may do with `~/.Trash`. Listing `~/.Trash` and reading `~/.Trash/.DS_Store` are denied. A rename into `~/.Trash`, a `stat` of a known name, and an unlink of a known name are allowed (`run1/tcc.tsv`, `run2/tcc.tsv`). So the sink can move items in and report their names, but it cannot list the Trash. That is why `NSFileManager` returning the landed URL matters: the sink learns the name without a directory read.

## Put Back

Put Back is not verified. Finder keeps put-back records in `~/.Trash/.DS_Store`, which this process may not read. Reading the enabled state of **File > Put Back** through accessibility was not reliable unattended. In trial reads that were not committed, the item showed as enabled for a file moved by `renameatx_np`, which has no put-back record, because Finder was not the active app and its menu state was stale. The test for this question is to press **Put Back** and check that the file returns, which `putback.py` does. It needs Finder in front and nobody else at the keyboard.

## Over SSH

Remote Login is off on this machine, so no method ran over SSH. Which method needs an Aqua session follows from how each one works:

- Finder AppleScript needs a running Finder in the user's Aqua session and Automation consent for the calling process. An SSH session cannot show the consent prompt, so it fails until consent exists.
- `NSFileManager` and `/usr/bin/trash` call Foundation in the calling process and need no Finder. They are expected to work over SSH. Access to `~/.Trash` then depends on the "Allow full disk access for remote users" setting of Remote Login.
- `renameatx_np` is one system call. It works over SSH under the same privacy rules as above.

## Pending operator runs

1. To check Put Back, run `spikes/s2/run.sh` after you remove the cleanup line, or trash one file with each method by hand (`s2trash <cli|nsfm|rename|finder> PATH`). In Finder, open the Trash, select each item, and choose **File > Put Back**. Record which items return. `putback.py LIST_FILE` does the same through agent-desktop when Finder can stay in front.
2. To test SSH, turn on Remote Login on a second Mac, connect with `ssh`, and run `spikes/s2/run.sh results/ssh 1`. Save the output as `s2-rerun.txt`. Expect Finder to fail and the other three to pass.
3. To remove the leftover container folder, delete `~/Library/Containers/com.dustpan.spike.s2` in Finder and enter your password if Finder asks. If that fails, give Terminal Full Disk Access and run `rm -r ~/Library/Containers/com.dustpan.spike.s2`.

A failed Put Back for `NSFileManager` would not change the verdict, because v1 ships "Reveal in Trash" and defers restore (`docs/PLAN.md` Appendix B). A failed SSH run for `NSFileManager` would make the sink fall back to `renameatx_np` with its own unique-name rule when there is no Aqua session.

Verdict. Use `NSFileManager trashItemAtURL` through `objc2-foundation` for the macOS `Trash` sink, because it moved every item kind in about 1 ms, returns the landed name without listing the Trash, and picks a unique name itself; treat a container folder that `containermanagerd` owns as a permission failure that is never retried as a delete, and keep Put Back and SSH as operator checks.
