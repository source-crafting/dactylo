# dactylo

[![CI](https://github.com/source-crafting/dactylo/actions/workflows/ci.yml/badge.svg)](https://github.com/source-crafting/dactylo/actions/workflows/ci.yml)

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

Launch with no arguments:

```bash
dactylo
```

The first time, you'll see the setup screen — use ←/→ to change a value, Tab to
switch between duration and level, Enter to start, and `q` to quit. Your choice
is saved, so every later launch drops you **straight into a session** with your
last-used duration and level. To change them again, press **s** on the results
screen to reopen setup; from setup, Esc takes you back to your stats.

Or set duration and level explicitly with flags:

```bash
dactylo --time 60 --level 3
```

| Flag      | Range   | Default |
|-----------|---------|---------|
| `--time`  | 5–600 s | 60      |
| `--level` | 1–5     | 3       |

If you pass one flag, the other takes its default.

### During a session

- The timer starts on your **first keystroke**, not when the screen appears.
- **Backspace** corrects the previous character.
- **Esc** cancels the run and jumps to the results screen with your partial
  stats — a cancelled run is **not** saved to history.
- **Ctrl-C** exits dactylo immediately.

### Results screen

After time runs out — or after you cancel with Esc — you'll see a per-level
dashboard: tabs for levels 1–5 (the level you just played is marked `*`), that
level's session count and average/best, your latest run, and two charts plotting
**WPM** and **accuracy** across that level's sessions. Use **←/→** to switch the
level tab; the whole view follows. Press **Enter** to restart with the same
settings, **s** to change settings, or `q`/Esc to quit. (On a short terminal the
charts are replaced by a text summary.)

## Difficulty levels

Words are sampled from a frequency-ranked English word list embedded in the
binary (`assets/words-en.txt`). Higher levels widen the pool to include rarer and
longer words. The list is curated to contain real words — abbreviations, acronyms,
codes, brand and personal names, and other non-word tokens have been removed.

The file is named by language (`words-en.txt`) so the trainer can grow to support
other languages by adding e.g. `words-de.txt` in the future.

| Level | Word pool                          |
|-------|------------------------------------|
| 1     | ~200 most common short words       |
| 2     | top 1,000 words                    |
| 3     | top 3,000 words                    |
| 4     | top 7,000 words                    |
| 5     | full list (rare and long words)    |

Only lowercase real words of two or more letters are used.

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

## License

Released under the [MIT License](LICENSE).
