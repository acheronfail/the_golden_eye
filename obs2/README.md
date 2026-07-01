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

# run OBS with browser and Rust hot reload
just dev
```
