# The Golden Eye

## Development

System dependencies:

* Common: `rustup`, `nodejs`
* Debian: OBS (system or flatpak) + `libdbus-1-dev libssl-dev nasm opencv pkg-config`
* Arch Linux: OBS (system or flatpak) + `cmake nasm opencv`
* macOS:
    * `brew install cmake opencv`
    * Have OBS installed in `/Applications`

Setup `.env` file:

* Copy `.env.sample` to `.env`
* Add in variables

Get started:

```shell
# for the first version (uses nodejs and OBS' websocket API):
just run

# for the upcoming version (uses Rust and is an OBS plugin):
just obs
# for linux, it's recommended to use the flatpak version of OBS (this is because
# the non-flatpak version doesn't currently have the YouTube OAuth plugin which
# is required for the YouTube integration):
just obs-flatpak
```
