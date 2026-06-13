<p align="center">
  <picture>
    <source media="(prefers-color-scheme: dark)" srcset="docs/dactylo-logo.svg">
    <img src="docs/dactylo-logo-light.svg" alt="dactylo" width="420">
  </picture>
</p>

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
- **Focused view** — a fixed-width column shows three lines at a time; the cursor
  starts on the top line, then stays on the middle line as text scrolls.
- **Backspace to correct** — fix mistakes as you go (errors still count toward
  accuracy).
- **End-of-session stats** — WPM, raw WPM, accuracy, errors, and consistency.
- **Progression tracking** — every session is compared against your average and
  personal best *for that level*, with ▲/▼ deltas.
- **Weakness tracking & practice** — dactylo records your weak keys, letter
  combinations, and fumbled words across sessions; press **w** to explore them
  and **Enter** to drill them in a blended practice run.
- **Persistent history** — results are stored as JSON lines in `~/.dactylo/`.

## Install

Requires a Rust toolchain ([rustup](https://rustup.rs)).

```bash
cargo install --git https://github.com/source-crafting/dactylo
```

This builds dactylo and installs the binary to `~/.cargo/bin/` (already on your
`PATH` with a standard rustup setup), so you can run `dactylo` from anywhere.

To build from a local checkout instead, run `cargo build --release` in the repo;
the binary is produced at `target/release/dactylo`.

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

- A slim header shows `dactylo` and `level N · m:ss` (the countdown) with a faint
  live WPM/accuracy readout; the text sits below a dotted rule.
- The timer starts on your **first keystroke**, not when the screen appears.
- **Backspace** corrects the previous character; the cursor is the highlighted box.
- **Esc** cancels the run and jumps to the results screen with your partial
  stats — a cancelled run is **not** saved to history.
- **Ctrl-C** exits dactylo immediately.

### Results screen

After time runs out — or after you cancel with Esc — you'll see a 2×3 grid:
**WPM**, **RAW**, **ACC** on top and **ERRORS**, **CONSIST**, **LEVEL** below,
each with a delta vs your average at that level — or a highlighted `best yet`
when you beat your record for that metric — and your level average under
**LEVEL**. Press **Enter** to restart, **s** for
settings, **h** for the history view, **w** for your weaknesses, or `q`/Esc to
quit.

### History view

Press **h** on the results screen for a per-level dashboard: tabs for levels 1–5
(the level you just played is marked `*`) and, for the selected level, a **WPM**
row and an **accuracy** row, each showing the latest/average/best plus a
sparkline of that metric across the level's sessions. Use **←/→** to switch the
level tab. Press **Esc** (or **h**) to return to the results screen, or `q` to
quit. (On a short terminal the sparklines are replaced by a text summary.)

### Weaknesses & practice

Press **w** on the results screen to open the weakness explorer: ranked columns
of your **weak keys**, **weak letter-combinations**, and most **fumbled words**,
each with an error rate and miss count, measured over your recent sessions. Use
**←/→** to switch the sort between error rate and total misses.

Press **Enter** there to start a **practice run** — a blended stream of the words
you fumble most plus words rich in your weak keys/combos. Practice updates your
weakness profile but is **not** saved to your WPM history or personal bests, so
drilling hard material never dents your records. From practice results, **Enter**
practices again and **w** reopens the explorer. (Mistake detail is stored
separately in `~/.dactylo/mistakes.jsonl`; until you've typed enough, the
explorer shows a "not enough data yet" note.)

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
