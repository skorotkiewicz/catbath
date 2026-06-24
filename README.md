# catbath

[![Build](https://img.shields.io/github/v/release/skorotkiewicz/catbath?style=flat-square)](https://github.com/skorotkiewicz/catbath/actions)
[![AUR](https://img.shields.io/aur/version/catbath?style=flat-square)](https://aur.archlinux.org/packages/catbath)
[![Size](https://img.shields.io/badge/size-464KB-blue?style=flat-square)](https://github.com/skorotkiewicz/catbath/releases)

a tiny terminal text editor in rust with search, undo, mouse. 

<img src="cath.png" height="180" />

## usage

```
catbath [-g|-w] <file>
```

- `-g` GUI (TUI)
- `-w` Web (browser editor)
- `ssh://user@host/path/file.txt` for remote editing

## keys

- `^Q` quit
- `^S` save
- `^Z` undo
- `^K` cut 
- `^U` paste
- `^F` search

## build

```sh
cargo build --release
# or
yay -S catbath
```
