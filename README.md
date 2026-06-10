# dactylo

A fast terminal typing trainer. Pick a duration and a difficulty level, type an
endless stream of words against the clock with live per-character color feedback,
and see how your speed, accuracy, and consistency compare to your own history.

Built in Rust with [ratatui](https://ratatui.rs).

## Features

- **Timed sessions** — choose 15, 30, 60, or 120 seconds.
- **Five difficulty levels** — drawn from progressively rarer/longer words.
- **Live feedback** — correct characters turn green, mistakes show the expected
  character on a red background, and the cursor marks your position.
- **Focused view** — a centered, fixed-width column shows three lines at a time;
  the cursor starts on the top line, then stays on the middle line as text scrolls.
- **Backspace to correct** — fix mistakes as you go (errors still count toward
  accuracy).
- **End-of-session stats** — WPM, raw WPM, accuracy, errors, and consistency.
- **Progression tracking** — every session is compared against your average and
  personal best *for that level*, with ▲/▼ deltas.
- **Persistent history** — results are stored as JSON lines in `~/.dactylo/`.

## Install

Requires a Rust toolchain ([rustup](https://rustup.rs)).

```bash
cargo build --release
```

The binary is produced at `target/release/dactylo`. Copy it somewhere on your
`PATH` (e.g. `cp target/release/dactylo ~/.local/bin/`) or run it in place.

## Usage

Launch with no arguments for the interactive setup screen:

```bash
dactylo
```

Use ←/→ to change a value, Tab to switch between duration and level, Enter to
start, and `q` to quit.

Or skip the setup screen with flags:

```bash
dactylo --time 60 --level 3
```

| Flag      | Range   | Default |
|-----------|---------|---------|
| `--time`  | 5–600 s | 60      |
| `--level` | 1–5     | 3       |

If you pass one flag, the other takes its default.

### During a session

- A 3·2·1 countdown precedes each run.
- The timer starts on your **first keystroke**, not when the screen appears.
- **Backspace** corrects the previous character.
- **Esc** or **Ctrl-C** aborts the session without recording it.

### Results screen

After time runs out you'll see your stats and how they compare to past sessions
at the same level. Press `r` to retry with the same settings, or `q`/Esc to quit.

## Difficulty levels

Words are sampled from a frequency-ranked English word list embedded in the
binary. Higher levels widen the pool to include rarer and longer words.

| Level | Word pool                          |
|-------|------------------------------------|
| 1     | ~200 most common short words       |
| 2     | top 1,000 words                    |
| 3     | top 3,000 words                    |
| 4     | top 7,000 words                    |
| 5     | full list (rare and long words)    |

Only lowercase words of two or more letters are used.

## Stats

| Stat            | Meaning                                                        |
|-----------------|----------------------------------------------------------------|
| **WPM**         | Net words per minute: `(correct chars / 5) / minutes`          |
| **Raw WPM**     | Same, counting every keystroke regardless of correctness       |
| **Accuracy**    | Percentage of keystrokes that were correct                     |
| **Errors**      | Total incorrect keystrokes (not reduced by backspacing)        |
| **Consistency** | How steady your speed was, from per-second WPM samples (0–100) |

## Data storage

Sessions are appended to `~/.dactylo/history.jsonl`, one JSON object per line:

```json
{"ts":"2026-06-10T14:03:11Z","duration":60,"level":3,"wpm":58.2,"raw_wpm":61.0,"accuracy":97.1,"errors":7,"consistency":88.4,"chars":291}
```

The directory is created on first run. Aborted sessions are not recorded, and
malformed lines are skipped (with a notice on the results screen) rather than
causing a crash.

## Development

```bash
cargo test     # run the test suite
cargo fmt      # format
cargo clippy --all-targets -- -D warnings
```

The design spec and implementation plan live under `docs/superpowers/`.

## License

Not yet specified.
