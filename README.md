# page

A simple, modern terminal pager written in Rust.

## Features

- Full ANSI SGR styling. Supporting bold, dim, italic, underline, reverse, 16 standard colors, 256-color palette, and 24-bit RGB truecolor. Styling is tracked across wrapped screen rows.
- Full Unicode support with proper column width calculation, including CJK characters and emoji. Supports soft (breaking on whitespace boundaries) and hard wrapping, as well as configurable tab stops.
- "No-wrap" mode to disable wrapping with support for horizontal scrolling.
- Line numbers in the gutter and a help screen to show keymap.
- Regex and Substring (default) searching with Smart (default), Sensitive and Insensitive case, and search highlighting.

## Installation

### Prerequisites

- [Rust](https://www.rust-lang.org/tools/install) (2024 edition)

### Installing

Install directly into `~/.cargo/bin` (or wherever `$CARGO_HOME/bin` is):

```bash
cargo install --git https://github.com/spaceavenue/page.git
```

Or inside a cloned repo:

```bash
cargo install --path .
```

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

### Testing

```bash
cargo test
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

# Disable ANSI escape processing
page --no-ansi styled_output.txt

# Custom tab width
page --tab-width 4 source_code.rs
```

### Command-Line Options

|Short| Long              | Description                                                  | Default |
|-----|-------------------|--------------------------------------------------------------|---------|
|`-f` | `--follow`        | Follow mode (scroll as new input arrives on stdin)           | `false` |
|`-w` | `--wrap <MODE>`   | Line wrap strategy: `soft` (word boundary), `hard`, or `off` | `soft`  |
|`-W` | `--no-wrap`       | Alias for `--wrap off`                                       | `false` |
|     | `--no-ansi`       | Disable ANSI/SGR escape codes parsing                        | `false` |
|`-N` | `--line-numbers`  | Show line numbers in gutter                                  | `false` |
|`-t` | `--tab-width <N>` | Number of columns between tab stops (1–256)                  | `8`     |
|`-c` | `--case <CASE>`   | Case sensitivity of the search: `smart`, `match`, `ignore`   | `smart` |
|`-s` | `--case match`    | Alias for `--case match`                                     | -       |
|`-i` | `--case ignore`   | Alias for `--case ignore`                                    | -       |
|`-S` | `--case smart`    | Alias for `--case smart`                                     | -       |
|`-e` | `--regex`         | Set initial search mode to regex                             | `false` |
|`-l` | `--literal`       | Set initial search mode to literal                           | `true`  |
|`-h` | `--help`          | Print help                                                   | -       |

### Keybinds

| Key                             | Action                        |
|---------------------------------|-------------------------------|
| `j` / `Down` / `Enter`          | Scroll down one row           |
| `k` / `Up`                      | Scroll up one row             |
| `Ctrl-D`                        | Scroll down half a page       |
| `Ctrl-U`                        | Scroll up half a page         |
| `Space` / `PageDown` / `Ctrl-F` | Scroll down one page          |
| `PageUp` / `Ctrl-B`             | Scroll up one page            |
| `g` / `Home`                    | Jump to beginning of document |
| `G` / `End`                     | Jump to end of document       |
| `Left` / `Right`                | Scroll left/right (no-wrap)   |
| `0`                             | Reset horizontal scroll       |
| `/`                             | Start search                  |
| `h` / `?`                       | Show the keymap               |
| `q` / `Esc` / `Ctrl-C`          | Exit the pager                |

## Architecture

- **`buffer`**: Owns data sources (memory-mapped files >= 64 KiB, heap buffer for small files, streaming background reader for `stdin`). Manages lazy newline index vectors.
- **`text`**: Applies tab expansion, Unicode display widths, lazy row iteration, wrapping and styling to create a `ParsedLine`.
  - `text::reader`: Zero-copy cursor over `&str`.
  - `text::ansi`: ECMA-48 SGR parameter parser emitting color and attribute changes.
  - `text::wrap`: Line wrapping implementation.
- **`scroll`**: Decoupled scroll state (`Anchor { line, row }`). Handles multi-row visual scrolling and boundary clamping.
- **`ui`**: Terminal alternate screen setup, viewport/layout math, theme colors and the status bar template.
- **`config`**: CLI parsing, pager options and action keymap.
- **`pager`**: Main terminal UI event loop driven by `crossterm`.

## License

This project is licensed under the [MIT License](LICENSE).
