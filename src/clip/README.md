# clip

This folder contains code for `fleximenu_clip(1)`, a clipboard history
that is picked from with FlexiMenu. It replaces the old `test.sh` script.

Usage:
- `fleximenu_clip add` copies the primary selection to the clipboard and adds it to history
- `fleximenu_clip out` copies stdin to the clipboard and adds it to history
- `fleximenu_clip sel` picks an entry from history with `fleximenu` and copies it to the clipboard

History is stored in `$HOME/.cache/cliphist`, one entry per line. Multiline
entries are stored with `<NEWLINE>` in place of each newline.

It needs `xclip` and `fleximenu` at run time. Notifications are sent over D-Bus
directly, so a notification daemon is needed to see them.
