# Setup

```sh
# install rustup
# install nodejs (version from .nvmrc)

# macos dependencies
xcode-select --install
brew install just cmake llvm opencv simde

# installs all dependencies
just setup
```

Now that's complete, you can:

```sh
# run the project and automatically start recording when a level starts
# and save the recording when the level is successfully cleared:
just run

# run OBS with the plugin
just obs
```
