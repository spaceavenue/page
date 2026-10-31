# page-rs

A terminal pager written in Rust.

Designed for near-instant startup and zero binary bloat.

---

## Features

- **Memory mapped file loading:** Files larger than 64 KiB are memory-mapped for near-instant loading.
- **Lazy On-Demand Indexing:** Newlines are indexed on demand using SIMD-accelerated scanning. Lines are only scanned to fill the viewport, the remainder is indexed when scrolling or jumping to the bottom.
- **Concurrent Streaming Stdin:** Piped input is ingested on a background thread in expanding chunks (8 KiB up to 256 KiB).
- **Full ANSI & Truecolor Styling:** SGR parser supporting bold, dim, italic, underline, reverse video, 16 standard colors, 256-color palette, and 24-bit RGB truecolor. Active styling is tracked across wrapped screen rows.
- **Unicode & Smart Word Wrapping:** Proper column width calculation (including CJK fullwidth characters and emoji). Supports soft (breaking on whitespace boundaries) and hard wrapping, as well as configurable tab stops.

---

## Installation

### Prerequisites

- [Rust](https://www.rust-lang.org/tools/install) (2024 edition / stable toolchain)

### Building

```bash
# Clone the repository
git clone https://github.com/spaceavenue/page-rs.git
cd page-rs

# Build an optimized release binary
cargo build --release

# The compiled binary is located at target/release/page
./target/release/page --help
```

### Installing

To install `page` directly into your `~/.cargo/bin` (or wherever `$CARGO_HOME` is):

```bash
cargo install --path .
```

---

## Usage

```bash
# View a file
page path/to/large_file.log

# Stream from standard input
dmesg -w | page

# Explicit stdin indicator
cat access.log | page -

# Follow growing streaming input (like tail -f)
journalctl -f | page -f

# Wrap strictly at terminal width (hard wrap)
page --wrap hard long_lines.txt

# Disable ANSI color escape processing
page -R styled_output.txt

# Custom tab width
page --tab-width 4 source_code.rs
```

### Command-Line Options

| Option | Short | Description | Default |
|--------|-------|-------------|---------|
| `--follow` | `-f` | Follow mode (scroll as new input arrives on stdin) | `false` |
| `--wrap <MODE>` | `-w` | Line wrap strategy: `soft` (word boundary) or `hard` | `soft` |
| `--no-ansi` | `-R` | Disable interpretation of ANSI/SGR escape codes | `false` (ANSI enabled) |
| `--tab-width <N>` | `-t` | Number of columns between tab stops (1–256) | `8` |
| `--help` | `-h` | Print help information | - |

---

## Keybinds

| Key | Action | Description |
|-----|--------|-------------|
| `j` / `Down` / `Enter` | `LineDown` | Scroll down by one row |
| `k` / `Up` | `LineUp` | Scroll up by one row |
| `Ctrl-D` | `HalfPageDown` | Scroll down by half a page |
| `Ctrl-U` | `HalfPageUp` | Scroll up by half a page |
| `Space` / `PageDown` / `Ctrl-F` | `PageDown` | Scroll down by one full page |
| `PageUp` / `Ctrl-B` | `PageUp` | Scroll up by one full page |
| `g` / `Home` | `GotoTop` | Jump to the very beginning of the document |
| `G` / `End` | `GotoBottom` | Jump to the end of the document |
| `q` / `Esc` | `Quit` | Exit the pager |

---

## Architecture Overview

- **`buffer`**: Owns data sources (`BufferSource::Mmap` for files $\ge$ 64 KiB, `BufferSource::Heap` for small files, and streaming background reader for `stdin`). Manages lazy newline index vectors.
- **`reader`**: Zero-copy cursor over `&str`.
- **`ansi`**: ECMA-48 SGR parameter parser emitting color and attribute changes.
- **`text`**: Handles tab expansions, Unicode display widths, and lazy row iteration. Preserves ANSI styling across wrapped lines.
- **`scroll`**: Decoupled scroll state (`Anchor { line, row }`). Handles multi-row visual scrolling and boundary clamping.
- **`pager`**: Main terminal UI event loop driven by `crossterm`.
**`action`**: Static keymap for actions.

---

## License

This project is licensed under the [MIT License](LICENSE).
