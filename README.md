# page

A simple, modern terminal pager written in Rust.

## Installation

### Prerequisites

- [Rust](https://www.rust-lang.org/tools/install) (2024 edition)

### Building

```bash
# Clone the repository
git clone https://github.com/spaceavenue/page.git
cd page

# Build a release binary
cargo build --release

# Binary is accessible at ./target/release/page
target/release/page --help
```

### Installing

To install `page` directly into your `~/.cargo/bin` (or wherever `$CARGO_HOME/bin` is):

```bash
cargo install --path .
```

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

# Show line numbers
page -N source_code.rs

# Chop long lines and scroll horizontally
page -W long_lines.txt

# Disable ANSI color escape processing
page -R styled_output.txt

# Custom tab width
page --tab-width 4 source_code.rs
```

### Command-Line Options

| Option | Short | Description | Default |
|--------|-------|-------------|---------|
| `--follow` | `-f` | Follow mode (scroll as new input arrives on stdin) | `false` |
| `--wrap <MODE>` | `-w` | Line wrap strategy: `soft` (word boundary), `hard`, or `off` | `soft` |
| `--no-wrap` | `-W` | Alias for `--wrap off` | `false` |
| `--no-ansi` | `-R` | Disable interpretation of ANSI/SGR escape codes | `false` (ANSI enabled) |
| `--line-numbers` | `-N` | Show a line-number gutter | `false` |
| `--tab-width <N>` | `-t` | Number of columns between tab stops (1–256) | `8` |
| `--help` | `-h` | Print help information | - |

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
| `Left` / `Right` | `ScrollLeft` / `ScrollRight` | Move horizontally in chop mode |
| `0` | `ScrollHome` | Reset horizontal scroll |
| `h` / `?` | `Help` | Show the keymap |
| `q` / `Esc` / `Ctrl-C` | `Quit` | Exit the pager |

## Testing

```bash
cargo test
```

## Features

- **Full ANSI & Truecolor Styling:** SGR parser supporting bold, dim, italic, underline, reverse video, 16 standard colors, 256-color palette, and 24-bit RGB truecolor. Active styling is tracked across wrapped screen rows.
- **Unicode & Smart Word Wrapping:** Proper column width calculation (including CJK fullwidth characters and emoji). Supports soft (breaking on whitespace boundaries) and hard wrapping, as well as configurable tab stops.
- **No-wrap mode with horizontal scrolling:** Disable wrapping. The left/right arrows move the visible column window.
- **Line numbers and a help screen:** Line numbers in the gutter and a help screen to show keymap.
- **Search Capabilities:** Both Regex and Substring (default) searching, with Smart (default), Sensitive and Insensitive case and search highlighting.
---

## Architecture

- **`buffer`**: Owns data sources (memory-mapped files $\ge$ 64 KiB, heap buffer for small files, streaming background reader for `stdin`). Manages lazy newline index vectors.
- **`text`**: Applies tab expansion, Unicode display widths, lazy row iteration, wrapping and styling, to create a `ParsedLine`.
  - `text::reader`: Zero-copy cursor over `&str`.
  - `text::ansi`: ECMA-48 SGR parameter parser emitting color and attribute changes.
  - `text::wrap`: Line wrapping implementation.
- **`scroll`**: Decoupled scroll state (`Anchor { line, row }`). Handles multi-row visual scrolling and boundary clamping.
- **`ui`**: Terminal alternate screen setup, viewport/layout math, theme colors and the status bar template.
- **`config`**: CLI parsing, pager options and action keymap.
- **`pager`**: Main terminal UI event loop driven by `crossterm`.

## License

This project is licensed under the [MIT License](LICENSE).
