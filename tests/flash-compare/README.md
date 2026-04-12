# ruffle-flash-compare

A tool that runs every SWF under `tests/tests/swfs/` through Adobe's real
`flashplayerdebugger`, captures the `trace()` output, and diffs it against
each test's committed `output.txt`. The result is an audit report showing
where Ruffle's expectations match Flash and where they drift.

This is a comparison/audit tool, not a regenerator — it never writes to
`output.txt` files.

## Prerequisites (Linux only)

1. Install Adobe Flash Player standalone debugger (`flashplayerdebugger`).
2. Export its path:
   ```
   export FLASHPLAYER_DEBUGGER=/path/to/flashplayerdebugger
   ```
3. Create `$HOME/mm.cfg` with at minimum:
   ```
   ErrorReportingEnable=1
   TraceOutputFileEnable=1
   MaxWarnings=0
   ```
   Without these keys, `flashplayerdebugger` will not write to
   `$HOME/.macromedia/Flash_Player/Logs/flashlog.txt` and every test will
   be reported as `NO_OUTPUT`.
4. You need an X/Wayland display — `flashplayerdebugger` is a GUI app.
   Alternatively, pass `--headless` and the tool will launch it under
   `xvfb-run -a` (requires `xvfb` installed).

## Running

From the repository root:

```
cargo run -p ruffle-flash-compare --release -- [flags]
```

Flags:

- `--filter <substring>` — only run tests whose name contains this substring.
- `--limit <N>` — cap the number of tests (handy for smoke tests).
- `--timeout-ms <ms>` — override the per-test kill timeout (default is
  derived from `num_frames * tick_rate`, clamped to `[3000, 60000]`).
- `--swfs-dir <path>` — override test root (default: `tests/tests/swfs`).
- `--report-dir <path>` — override output directory
  (default: `tests/flash-compare/report`).
- `--headless` — run `flashplayerdebugger` under `xvfb-run -a` so no real
  display is needed (CI-friendly).

## Output

Written to `--report-dir`:

- `results.json` — machine-readable per-test results.
- `summary.md` — counts and a diff table.
- `diffs/<test_name>.diff` — per-test textual diff for every mismatched test.

## Limitations

- Tests that require Ruffle input injection (`input.json`) or a mock socket
  server (`socket.json`) are skipped — `flashplayerdebugger` cannot drive
  them.
- Runs are sequential because `flashplayerdebugger` writes to a single
  global `flashlog.txt`; parallelism would interleave outputs.
- Most test SWFs do not call `fscommand("quit")`, so the tool kills the
  player after a derived timeout. If you see truncated `actual` output,
  raise `--timeout-ms`.
- Player version (`player_options.version` in `test.toml`) is not enforced
  — whatever version your `flashplayerdebugger` is, that's what runs.
- Linux only. macOS/Windows have different `mm.cfg` and log-file paths and
  are not supported.
