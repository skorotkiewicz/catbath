# catbath

[![Build](https://img.shields.io/github/v/release/skorotkiewicz/catbath?style=flat-square)](https://github.com/skorotkiewicz/catbath/actions)
[![AUR](https://img.shields.io/aur/version/catbath?style=flat-square)](https://github.com/skorotkiewicz/catbath/releases)
[![Size](https://img.shields.io/badge/size-464KB-blue?style=flat-square)](https://github.com/skorotkiewicz/catbath/releases)

A tiny terminal text editor in Rust. Single binary, zero dependencies.

<img src="cath.png" height="180" />

## Usage

```
editor [-g|-w] <file>
```

- `-g` GUI mode (TUI provided)
- `-w` Web mode (browser editor)
- `ssh://user@host/path/file.txt` for remote editing

## Keys

- `^Q` quit
- `^S` save
- `^Z` undo
- `^K` cut line (repeat for multi-line)
- `^U` paste cut lines
- `^F` search

## Build

```sh
cargo build --release
# or
yay -S catbath
```
