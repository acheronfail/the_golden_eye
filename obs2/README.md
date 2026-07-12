# Setup

```sh
# install rustup
# install nodejs (version from .nvmrc)

# macos dependencies
xcode-select --install
brew install just cmake wget simde nasm

# installs all dependencies
just setup
```

Now that's complete, you can:

```sh
# run OBS with the plugin
just obs

# run OBS with the browser dev server + hot-reloading the core into the
# running OBS session on Rust changes, no restart needed (see obs2/scripts/dev.py)
just dev
```
