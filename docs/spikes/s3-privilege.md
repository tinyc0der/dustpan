# S3: privilege helper handoff

Spike S3 asks whether the TUI can suspend itself, run `sudo -v` with Touch ID, start `dustpan --privileged-helper` over a pipe, and resume without leaking keystrokes. It also asks what the Windows `runas` relaunch needs. The answer sets the `Privileged` sink and the shape of M4.

- Branch: `spike/s3-privilege` at `c54dd6a6c2c3eda55f504f9f926e6f6027910287`.
- Program: `spikes/s3/src/main.rs` (`s3priv`, ratatui 0.29 plus crossterm 0.28).
- Harness: `spikes/s3/harness.py`, run by `spikes/s3/run.sh`.
- Outputs: `spikes/s3/results/run1/` and `run2/`.

## What the program does

`s3priv` draws a one-box TUI. Pressing `a` runs the admin path. Pressing `u` runs the same path without sudo.

1. It leaves the alternate screen and raw mode, so the terminal is in its normal state.
2. It runs `/usr/bin/sudo -v` with the terminal's own stdin, stdout and stderr. Touch ID or the password prompt belongs to sudo.
3. On success it starts `sudo -n <own exe> --privileged-helper` with stdin and stdout piped. The `-n` flag means the helper never prompts. It reuses the credential that step 2 cached.
4. It writes one line, `DELETE<TAB><path>`, closes the pipe, and reads one reply line.
5. It discards pending terminal input with `tcflush(TCIFLUSH)`. It re-enters raw mode and the alternate screen, then reads and drops any crossterm event that is already queued.

The helper accepts one closed action, `DELETE`. It deletes only a regular file whose parent folder is named `dustpan-s3`, and it replies `OK` or `ERR` with its effective uid. No environment variable or argument widens what it deletes.

## Unattended results

The harness runs `s3priv --auto unpriv` in a pseudo-terminal. A stand-in for `sudo -v` turns echo off, prints `Password:`, reads a line from `/dev/tty`, and allows three tries, which is what sudo does. Unlike sudo, it does not flush type-ahead when it restores the terminal, so the test leaves more stray keys for the TUI than real sudo would. The harness types a wrong password, waits for `Sorry, try again.`, and then types the right password and the stray keys `xyz` in one write, the way a user types ahead.

| Case | Helper reply | Target after | Stray keys after resume | Alt screen enter / leave |
|---|---|---|---|---|
| `drain` | `OK deleted uid=501` | gone | 0 | 2 / 2 |
| `no-drain-control` (`--no-drain`) | `OK deleted uid=501` | gone | 3 (`x`, `y`, `z`) | 2 / 2 |
| `refuse-outside-fixture` | `ERR refused: not in a dustpan-s3 fixture folder` | kept | 0 | 2 / 2 |

The control case proves the check can see a leak. Without the flush, the keys typed during the prompt arrive in the TUI as three key events. With the flush, none arrive. No password text appears in the pseudo-terminal output in any case (`password_echoed False`).

Both runs give the same values except `elapsed_ms` (337, 345 and 345 ms in run 1). `FIONREAD` reported 0 pending bytes before the flush in every case. A pseudo-terminal in canonical mode does not count a partial line, so the count is not a usable signal. Use the flush and the event drain, not the count.

## Pending operator runs

Touch ID, a real sudo password and a Windows UAC click cannot run unattended. Do these three runs in one session. Each one needs a root-owned fixture.

1. Build the program with `cargo build --release --manifest-path spikes/s3/Cargo.toml`.
2. Create the fixture with `sudo mkdir -p /private/var/tmp/dustpan-s3 && sudo install -o root -g wheel -m 0644 /dev/null /private/var/tmp/dustpan-s3/root-owned.txt`.
3. To test Touch ID, run `sudo -k`, then start `spikes/s3/target/release/s3priv` in Terminal.app at 120 by 40. Press `a`. Touch the sensor once. Type a few letters while the prompt is up. After the TUI comes back, the status line must say `helper replied OK deleted uid=0`, and no `key ...` line may appear. Save a screenshot as `docs/spikes/media/M0-2-review-s3-touchid.png`.
4. Create the fixture again (step 2). Run `sudo -k`. Start `s3priv` and press `a`. Cancel the Touch ID sheet to get the password prompt, or use a session without Touch ID. Type a wrong password, then the right one, then a few letters before the TUI returns. The checks are the same as in step 3. Save `docs/spikes/media/M0-2-review-s3-password.png`.
5. Record 30 to 60 seconds of step 3 or 4 as `docs/spikes/media/M0-2-review.mp4`.

For a file report instead of the status line, run `s3priv --auto sudo --target /private/var/tmp/dustpan-s3/root-owned.txt --report s3-sudo.tsv`. The report has the same fields as the unattended runs. A result different from the unattended path changes the verdict only if sudo itself reads keys after `sudo -v` returns, or if one fingerprint does not cover both `sudo -v` and `sudo -n`.

## What the Windows `runas` relaunch needs

This part is written from the Win32 API contract. It has not run, because it needs a Windows machine and a UAC click.

- Start the helper with `ShellExecuteExW`, `lpVerb = "runas"`, `lpFile` set to the dustpan executable, and `fMask = SEE_MASK_NOCLOSEPROCESS`. Pass `nShow = SW_HIDE` so the helper's console window stays hidden. One call gives one UAC prompt.
- An elevated process does not inherit handles from a non-elevated parent, so stdin and stdout pipes do not work. The parent creates a named pipe `\\.\pipe\dustpan-<random>` whose DACL grants access only to the current user's SID. It passes the name and its own process id on the helper's command line.
- The helper connects as the client and checks the server with `GetNamedPipeServerProcessId`. The parent checks the client with `GetNamedPipeClientProcessId`, then confirms that the client image path is its own executable and that the client token is elevated.
- If the user cancels the prompt, `ShellExecuteExW` fails with `ERROR_CANCELLED` (1223). Map that to "admin declined", not to an error.
- The parent waits on `hProcess` and reads the exit code with `GetExitCodeProcess`. The helper exits after its last action, so a session batches every admin action through one helper and one prompt.
- The UAC prompt runs on the secure desktop, so typed keys do not reach the console. Still call `FlushConsoleInputBuffer` before the TUI resumes.

Lane 6 checks this on Windows. It passes when one UAC prompt appears, the helper exits after its action, and `s3-windows.png` shows both.

Verdict. Build the `Privileged` sink as suspend, `sudo -v` on the user's terminal, then `sudo -n dustpan --privileged-helper` over stdin and stdout with a closed action enum, and always `tcflush` plus drain events before resume; Touch ID and password runs are pending the operator, and Windows uses a `runas` relaunch over an ACL-locked named pipe.
