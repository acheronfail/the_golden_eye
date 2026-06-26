# The Golden Eye

error: os_dlopen(/home/acheronfail/src/ge-obs/obs2/build/the_golden_eye.so->/home/acheronfail/src/ge-obs/obs2/build/the_golden_eye.so): libopencv_imgcodecs.so.410: cannot open shared object file: No such file or directory


## Development

System dependencies:

* Common: `rustup`, `nodejs`
* Debian: `libdbus-1-dev libssl-dev opencv pkg-config`
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
# for linux, it's recommended to use the flatpak version of OBS (this is because
# the non-flatpak version doesn't currently have the YouTube OAuth plugin which
# is required for the YouTube integration):
just obs-flatpak
```
