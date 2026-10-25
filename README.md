# FlexiMenu

A dynamic menu for X, written in Rust. It reads newline-separated items
on stdin, narrows them as you type, and prints the one you pick, so it
works anywhere a dmenu-style menu does. Plugins add new behavior, such
as a calculator or a power menu.

## Features

- **Placement**: a bar along the top or bottom of the screen (`-b`),
  centered (`-c`), or at the mouse pointer (`-M`). Centered and pointer
  menus are as wide as their items; `-z` sets an exact width and
  `--minwidth` the smallest one.
- **Vertical lists** with `-l`, and taller rows with `-h`.
- **Colored icon boxes**: an item starting with `:r `, `:g `, `:y `,
  `:b `, `:p `, `:h ` or `:s ` has its icon boxed in that color while
  it's selected (the `emojihighlight` plugin).
- **Power menu**: `--power` shows lock, logout, suspend, hibernate,
  reboot and shutdown, and runs the one you pick.
- **Plugins** for fuzzy matching, a calculator, spell checking,
  password entry and more, chosen at build time.

## Requirements

- Xlib headers
- A Rust toolchain (cargo and rustc)
- A C compiler

## Build and install

Edit `config.mk` for your system if needed. It installs under
`/usr/local` by default and enables the `emojihighlight` and `power`
plugins. Then:

```sh
make
sudo make install
```

This installs `fleximenu`, the `fleximenu_run` launcher and its
`fleximenu_path` helper, `stest`, and the `fleximenu` and `stest` man
pages.

## Usage

```sh
printf 'one\ntwo\nthree\n' | fleximenu -l 3 -p 'Pick:'
fleximenu_run                 # pick a program from $PATH and run it
fleximenu --power -c          # centered power menu
```

`man fleximenu` lists every option, and `make test` opens a quick test
menu.

## Plugins

Plugins are set with `PLUGINS` in `config.mk`. Run `make plugins` to
list the available ones and what they do, and see the
[plugin guide](src/plugins/README.md) to write your own.

## License

GPL-3.0; see [LICENSE](LICENSE).
