# catbath

[![Build](https://github.com/catbath/catbath/actions/workflows/rust.yml/badge.svg)](https://github.com/catbath/catbath/actions)
[![AUR](https://img.shields.io/aur/version/catbath?style=flat-square)](https://github.com/catbath/catbath/releases)
[![Size](https://img.shields.io/badge/size-464KB-blue?style=flat-square)](https://github.com/catbath/catbath/releases)

A tiny terminal text editor in Rust. Single binary, zero dependencies.

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

```
cargo build --release
```
