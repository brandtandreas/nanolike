# nanolike

A nano-inspired terminal text editor written in Rust.

## Features

- Multi-file editing with a tab bar
- Text selection with shift+navigation keys
- Encoding detection and round-trip encoding support (UTF-8, UTF-16, Latin-1, Windows-125x, GBK, and more)
- Search and replace (case-insensitive, with match counter and wraparound)
- Full undo/redo (200-step history per tab)
- Auto-indent and configurable tab handling
- Line numbers (toggleable)
- Customisable keybindings via JSON

## Installation

Requires [Rust](https://rustup.rs/) (edition 2021 or later).

```sh
git clone <repo>
cd nanolike
cargo build --release
# binary is at target/release/nanolike
```

## Usage

```
nanolike [OPTIONS] [FILE...]
```

Open one or more files in separate tabs:

```sh
nanolike file.txt
nanolike src/main.rs src/lib.rs
```

### Options

| Flag | Description |
|---|---|
| `--help`, `-h` | Show help and exit |
| `--export-config` | Write the default config file to `~/.config/nanolike/config.json` and exit |
| `--export-keybindings` | Write the default keybindings file to `~/.config/nanolike/keybindings.json` and exit |

## Interface

```
┌──────────────────────────────────────────────────────────────────────┐
│  main.rs │ *lib.rs │                                       NanoLike  │  ← tab bar
├──────────────────────────────────────────────────────────────────────┤
│  1  fn main() {                                                       │
│  2      let x = 42;                                                   │  ← text area
│  3  }                                                                 │
│  ~                                                                    │
├──────────────────────────────────────────────────────────────────────┤
│  Saved: main.rs [UTF-8]                       Ln 1, Col 1 | UTF-8    │  ← status bar
├──────────────────────────────────────────────────────────────────────┤
│  ^X Quit │ ^S Save │ ^F Search │ ^R Replace │ ^L GoTo │ ^Z Undo      │  ← help rows
│  ^K Cut  │ ^C Copy │ ^V Paste  │ ^A Sel All │ ^Y Redo │ ^O Open     │
└──────────────────────────────────────────────────────────────────────┘
```

- **Tab bar** — active tab is shown in white/bold; a `*` prefix marks unsaved changes; inactive modified tabs are highlighted in yellow.
- **Status bar** — shows the last status message on the left and cursor position plus file encoding on the right.
- **Help rows** — the two bottom rows list the most common shortcuts. Press `Ctrl+G` or `F1` for the full list.

## Keyboard Shortcuts

### File

| Key | Action |
|---|---|
| `Ctrl+S` / `Ctrl+O` | Save |
| `Ctrl+Shift+S` | Save as… |
| `Alt+O` | Open file (prompts for path, opens in new tab or reuses current empty tab) |
| `Ctrl+X` / `Ctrl+Q` | Quit (prompts to save any modified tabs) |

### Tabs

| Key | Action |
|---|---|
| `Ctrl+T` | New empty tab |
| `Alt+O` | Open file in tab |
| `Alt+.` | Next tab |
| `Alt+,` | Previous tab |
| `Ctrl+W` | Close current tab (prompts to save if modified) |

### Editing

| Key | Action |
|---|---|
| `Ctrl+Z` | Undo |
| `Ctrl+Y` / `Ctrl+Shift+Z` | Redo |
| `Ctrl+K` | Cut line (or selection) |
| `Alt+C` / `Ctrl+C` | Copy line (or selection) |
| `Ctrl+U` / `Ctrl+V` | Paste |
| `Ctrl+A` | Select all |
| `Tab` | Insert tab / spaces (see config) |
| `Ctrl+Backspace` / `Alt+Backspace` | Delete word before cursor |
| `Alt+D` | Delete to end of line |

### Navigation

| Key | Action |
|---|---|
| Arrow keys | Move cursor |
| `Ctrl+Left` / `Alt+Left` | Previous word |
| `Ctrl+Right` / `Alt+Right` | Next word |
| `Home` | Toggle between line start and first non-whitespace |
| `End` | End of line |
| `Ctrl+Home` | Top of file |
| `Ctrl+End` | Bottom of file |
| `Page Up` / `Page Down` | Scroll one screen |
| `Ctrl+L` | Go to line number |

### Selection

Hold `Shift` with any navigation key to extend the selection:

| Key | Action |
|---|---|
| `Shift+Arrow` | Extend selection by one character / line |
| `Shift+Ctrl+Left/Right` | Extend selection by word |
| `Shift+Home` / `Shift+End` | Extend selection to line start / end |
| `Shift+Ctrl+Home/End` | Extend selection to file start / end |
| `Shift+Page Up/Down` | Extend selection by one screen |
| `Escape` | Clear selection |

Typing or pressing `Backspace`/`Delete` while a selection is active replaces or removes the selected text.

### Search & Replace

| Key | Action |
|---|---|
| `Ctrl+F` / `Ctrl+W` | Search (case-insensitive; Enter to confirm, Escape to cancel) |
| `F3` | Next match |
| `Ctrl+P` | Previous match |
| `Ctrl+R` / `Ctrl+H` | Replace all occurrences |

Search matches are highlighted in yellow; the status bar shows the current match position (e.g. `Match 2/7`).

### Display

| Key | Action |
|---|---|
| `Alt+N` | Toggle line numbers |
| `Alt+W` | Toggle word wrap |
| `Alt+I` | Toggle auto-indent |
| `Ctrl+G` / `F1` | Show full help screen |

## Configuration

Config is stored at `~/.config/nanolike/config.json`. Run `nanolike --export-config` to create the file with all defaults:

```json
{
  "tab_size": 4,
  "use_spaces": true,
  "auto_indent": true,
  "word_wrap": false,
  "line_numbers": true
}
```

| Key | Type | Default | Description |
|---|---|---|---|
| `tab_size` | integer | `4` | Width of a tab stop and number of spaces inserted by `Tab` |
| `use_spaces` | bool | `true` | Insert spaces instead of a literal tab character |
| `auto_indent` | bool | `true` | New lines inherit the indentation of the previous line |
| `word_wrap` | bool | `false` | Soft-wrap long lines at the terminal width |
| `line_numbers` | bool | `true` | Show line numbers in the left gutter |

## Keybindings

Keybindings are stored at `~/.config/nanolike/keybindings.json`. Run `nanolike --export-keybindings` to create the file. Any entries you add override the defaults; unspecified actions keep their defaults.

Each action maps to a list of key strings:

```json
{
  "save": ["ctrl+s", "ctrl+o"],
  "quit": ["ctrl+x"]
}
```

Supported key formats: `ctrl+<key>`, `alt+<key>`, `ctrl+shift+<key>`, `shift+<key>`, `f1`–`f12`, `up`, `down`, `left`, `right`, `home`, `end`, `pageup`, `pagedown`, `backspace`, `delete`, `enter`, `escape`, `tab`.

### Available actions

| Action | Default keys |
|---|---|
| `quit` | `ctrl+x`, `ctrl+q` |
| `save` | `ctrl+s`, `ctrl+o` |
| `save_as` | `ctrl+shift+s` |
| `open_file` | `alt+o` |
| `new_tab` | `ctrl+t` |
| `next_tab` | `alt+.` |
| `prev_tab` | `alt+,` |
| `close_tab` | `ctrl+w` |
| `help` | `ctrl+g`, `f1` |
| `cut_line` | `ctrl+k` |
| `copy_line` | `alt+c`, `ctrl+c` |
| `paste` | `ctrl+u`, `ctrl+v` |
| `select_all` | `ctrl+a` |
| `undo` | `ctrl+z` |
| `redo` | `ctrl+y`, `ctrl+shift+z` |
| `search` | `ctrl+f`, `ctrl+w` |
| `search_next` | `f3` |
| `search_prev` | `ctrl+p` |
| `replace` | `ctrl+r`, `ctrl+h` |
| `goto_line` | `ctrl+l` |
| `page_up` | `pageup` |
| `page_down` | `pagedown` |
| `file_top` | `ctrl+home` |
| `file_bottom` | `ctrl+end` |
| `next_word` | `alt+right`, `ctrl+right` |
| `prev_word` | `alt+left`, `ctrl+left` |
| `delete_word` | `ctrl+backspace`, `alt+backspace` |
| `delete_to_eol` | `alt+d` |
| `toggle_line_numbers` | `alt+n` |
| `toggle_word_wrap` | `alt+w` |
| `toggle_auto_indent` | `alt+i` |

## Encoding

Files are decoded on open using BOM detection first, then statistical charset detection ([chardetng](https://github.com/hsivonen/chardetng), the same engine used by Firefox). The detected encoding is displayed in the status bar (e.g. `UTF-8`, `UTF-8 BOM`, `windows-1252`).

On save the file is re-encoded back to its original encoding. UTF-8 BOMs are preserved. UTF-16 LE/BE files are written back as UTF-16. New files are always saved as UTF-8.

## Dependencies

| Crate | Purpose |
|---|---|
| [crossterm](https://github.com/crossterm-rs/crossterm) | Cross-platform terminal control |
| [serde](https://serde.rs/) + serde_json | Config and keybinding serialisation |
| [dirs](https://github.com/dirs-dev/dirs-rs) | Locating the user config directory |
| [encoding_rs](https://github.com/nicowillis/encoding_rs) | Encoding/decoding non-UTF-8 files |
| [chardetng](https://github.com/hsivonen/chardetng) | Statistical charset detection |
