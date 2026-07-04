# The Golden Eye

This is a plugin for OBS Studio that assists with Goldeneye N64 speed-running.

## OBS compatibility

The recommended minimum OBS Studio version is `31.1.0`, but the plugin should work on OBS `31.0.0` and later.

## How to install

Follow the instructions for your operating system:

- [Install on Linux](docs/install-linux.md)
- [Install on macOS](docs/install-macos.md)
- [Install on Windows](docs/install-windows.md)

## Development

System dependencies:

* Common:
    * `rustup`, `nodejs` (version from `.nvmrc`), `just`, `wget`
* Linux:
    * Debian: `libdbus-1-dev libssl-dev nasm pkg-config`
    * Arch Linux: `cmake nasm pkg-config`
    * This project targets the flatpak release of OBS Studio, so you need flatpak installed, and:
        * OBS: `flatpak install com.obsproject.Studio`
        * SDK: `flatpak install $(flatpak info --show-sdk com.obsproject.Studio)`
* macOS:
    * `xcode-select --install`
    * `brew install just cmake wget simde nasm`
* Windows:
    * OBS Studio
    * Visual Studio Build Tools with MSVC
    * `vcpkg`, with `VCPKG_ROOT` or `VCPKG_INSTALLATION_ROOT` set
    * `just`, `cmake`, and Git Bash

Get started:

```shell
# run once after cloning this repository to install dependencies and set up the environment:
just setup

# build and run OBS with the native plugin
# (on linux this builds inside the OBS Flatpak SDK and runs the Flatpak OBS):
just obs

# development mode with browser hot reload and plugin core hot reload:
just dev

# release-build the plugin and run frame regression tests:
just test
```
