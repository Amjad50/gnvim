<p align="center">
	<img src="./desktop/gnvim_128.png" alt="GNvim Logo">
    <h3 align="center">GNvim - Rich Neovim GUI without any web bloat</h3>
</p>

[![Build Status](https://travis-ci.org/vhakulinen/gnvim.svg?branch=master)](https://travis-ci.org/vhakulinen/gnvim)
[![Gitter](https://badges.gitter.im/gnvim/community.svg)](https://gitter.im/gnvim/community?utm_source=badge&utm_medium=badge&utm_campaign=pr-badge)

GNvim, Neovim GUI aiming for rich code editing experience without any
unnecessary web bloat.

GNvim has been my daily driver since August 2018. I try to add new features as
I find time for it, and any help is welcome!

<p align="center">
	<img src="https://github.com/vhakulinen/gnvim/wiki/completionmenu.png" alt="Logo">
</p>

For more screenshots, see [the wiki](https://github.com/vhakulinen/gnvim/wiki).

TL;DR to get started on Ubuntu 18.04 after cloning this repo and assuming
you have [rust tool chain](https://rustup.rs/) installed:

```
$ sudo apt install libgtk-3-dev libwebkit2gtk-4.0-dev
$ # Run (unoptimized version) without installing
$ GNVIM_RUNTIME_PATH=/path/to/gnvim/runtime cargo run
$ # Install
$ make
$ sudo make install
```

## Features

* No electron (!), build on GTK.
* Ligatures
* Custom cursor tooltip feature to display markdown documents.
  Useful for implementing features like hover information or signature help
  (see [gnvim-lsp](https://github.com/vhakulinen/gnvim-lsp)).
* A lot of the nvim external features implemented
    - Popupmenu
        * Own view for `preview` (`:h completeopt`).
    - Tabline
    - Cmdline
    - Wildmenu

More externalized features will follow as they are implemented for neovim.

## Requirements

GNvim requires

* Stable rust to compile
* Latest nvim master (gnvim 0.1.0 works with nvim 0.3.4)
* Gtk version 3.18 or higher

On some systems, Gtk packages doesn't include development files. On Ubuntu
18.04, you'll need the following ones:

```
$ sudo apt install libgtk-3-dev libwebkit2gtk-4.0-dev
```

For other systems, see requirements listed by gtk-rs project [here](https://gtk-rs.org/docs-src/requirements.html).
Note that you'll need the `libwebkit2gtk-4.0-dev` package too.

There are some benchmarks for internal data structures, but to run those you'll
need nightly rust. To run those benchmarks, use `cargo bench --features=unstable`
command.

# Install

You're required to have rust tool chain available. Once you have that, clone
this repo and run `make build` followed by `sudo make install`.

# Running

TL;DR: Without installing:

```
GNVIM_RUNTIME_PATH=/path/to/gnvim/runtime cargo run
```

GNvim requires some runtime files to be present and loaded by Neovim to work
properly. By default, GNvim will look for these files in `/usr/local/share/gnvim/runtime`,
but this can be changed by the `GNVIM_RUNTIME_PATH` environment variable.

By default, GNvim will use `nvim` to run neovim. If you want to change that,
you can use `--nvim` flag (e.g. `gnvim --nvim=/path/to/nvim`).

For debugging purposes, there is `--print-nvim-cmd` flag to tell GNvim to print
the executed nvim command.

See `gnvim --help` for all the cli arguments.
