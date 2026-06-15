# todo_tui

## Why another todo app?

I found it surprisingly difficult to find an existing application that had all the features I wanted:

- **Lightweight**. No Electron etc. Startup time must be 0.0 seconds.
- **Frictionless**. No menus or modal dialogs. Every action should be a single keystroke (ideally with vim-like keybindings).
- **Human-readable and portable file format**. Plaintext or markdown preferred.
- **Nested subtasks**. It helps me to break tasks down into bite-sized pieces. This is the main feature I'm missing in the [todo.txt](http://todotxt.org/) format.
- **Highlight the first actionable step of each task**. Mitigates the overwhelm of a huge list of tasks.

So that's what `todo_tui` does.
It also auto-saves the file on every change and performs round-trip verification on load and save to ensure that the file contents are never corrupted by the program.

## Install

`git clone git@github.com:mario-holubar/todo_tui.git && cd todo_tui && cargo install --path .`

## Configure

See [default_config.toml](default_config.toml) for options.
