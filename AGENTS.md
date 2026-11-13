# lumen, for agents

[Contributing](site/src/content/docs/contributing.md) says how to build lumen and how its code reads. This is the short form.

## Checks

These run on every push and have to pass:

```sh
cargo fmt --all --check
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo clippy --all-targets --locked --features lag -- -D warnings
cargo test --workspace --locked
```

## Performance

Before changing anything to make lumen faster or smoother, measure, and measure again after. Never open a window on the user's desktop for it:

```sh
scripts/lag.sh                          # builds with --features lag, types a scripted scenario on a hidden display, prints the report
scripts/lag.py before.jsonl after.jsonl # compares two records, one per build or checkout
```

- The report splits the time from a key to its frame into the shell answering, waiting for a frame, drawing and handing over. It counts the frames each cursor glide gets and lists the main thread's pauses with their source line. "Measure lag" in Contributing says how to read it.
- `scripts/lag.sh` needs gamescope, and refuses to measure while `pulse load` says the machine is busy.
- The recorder is `src/lag.rs`: its events, their fields and the scripted scenario (`SCENARIO`). Add an event there when a question needs one.
- To compare against `main`, build it in a worktree of its own and run `scripts/lag.sh` there too.
- A record of the user's own typing needs a real window: they run `LUMEN_LAG=<file> target/release/lumen` from a `--features lag` build, and you read the file with `scripts/lag.py`.
- Frames about 33 ms apart mean GPUI is holding them back (a window without the focus, or the frame limits in `WindowOptions` in `src/main.rs`), not that drawing is slow.
