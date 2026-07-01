# The Golden Eye

## Development

System dependencies:

* Common: `rustup`, `nodejs` (version from `.nvmrc`), `just`, `wget`
* Debian: OBS (system or flatpak) + `libdbus-1-dev libssl-dev nasm pkg-config`
* Arch Linux: OBS (system or flatpak) + `cmake nasm pkg-config`
* macOS:
    * `xcode-select --install`
    * `brew install just cmake wget simde nasm`
    * Have OBS installed in `/Applications`

Get started:

```shell
just setup

# build and run OBS with the native plugin:
just obs

# for linux, it's recommended to use the flatpak version of OBS (this is because
# the non-flatpak version doesn't currently have the YouTube OAuth plugin which
# is required for the YouTube integration):
just obs-flatpak

# development mode with browser hot reload and plugin core hot reload:
just dev

# release-build the plugin and run frame regression tests:
just test
```
