# The Golden Eye

This is a plugin for OBS Studio that assists with Goldeneye N64 speed-running.

## How to install

Follow the instructions for your operating system:

- [Install on Linux](docs/install-linux.md)
- [Install on macOS](docs/install-macos.md)

## Development

System dependencies:

* Common:
    * `rustup`, `nodejs` (version from `.nvmrc`), `just`, `wget`
    * The flatpak SDK that OBS uses is needed: `flatpak install $(flatpak info --show-sdk com.obsproject.Studio)`
* Debian: OBS (system or flatpak) + `libdbus-1-dev libssl-dev nasm pkg-config`
* Arch Linux: OBS (system or flatpak) + `cmake nasm pkg-config`
* macOS:
    * `xcode-select --install`
    * `brew install just cmake wget simde nasm`
    * Have OBS installed in `/Applications`

Get started:

```shell
# run once after cloning this repository to install dependencies and set up the environment:
just setup

# build and run OBS with the native plugin:
just obs

# for linux, it's recommended to use the flatpak version of OBS (this is the recommended
# install method from the OBS developers, but it also includes the embedded browser which
# this plugin needs for its UI to work inside of OBS):
just obs-flatpak

# development mode with browser hot reload and plugin core hot reload:
just dev

# release-build the plugin and run frame regression tests:
just test
```
