# The Golden Eye

## Development

System dependencies:

* Common: `rustup`, `nodejs`
* Arch Linux: `cmake obs-studio opencv`
* macOS:
    * `brew install cmake opencv`
    * Have OBS installed in `/Applications`

Setup `.env` file:

* Copy `.env.sample` to `.env`
* Add in variables
* Arch Linux needs:
    * `OPENCV_INCLUDE_DIR="/usr/include/opencv4"`
    * `OPENCV_LIB_DIR="/usr/lib"`

Get started:

```shell
# for the first version (uses nodejs and OBS' websocket API):
just run

# for the upcoming version (uses Rust and is an OBS plugin):
just obs
```
