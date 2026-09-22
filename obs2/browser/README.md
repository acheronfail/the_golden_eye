# Browser dock

This SvelteKit application is the plugin's OBS browser dock. CMake builds it as one HTML bundle,
which Rust embeds. The project uses the static adapter.

Follow [the contribution guide](../../CONTRIBUTING.md) for setup, builds, checks, and tests.
Run these commands from the repository root:

```sh
just dev          # OBS backend plus Vite with hot reload
just storybook    # isolated component previews without OBS
just test-browser
just test-storybook
```

Within this directory, `npm run test:unit` runs the Node/jsdom suites in watch mode.
`npm run test:storybook` runs the Chromium stories in watch mode. Add `-- --run` for a single run.
`npm test` runs both suites once. Storybook tests check Chromium before starting Vitest.

`npm run check` checks Svelte and TypeScript. `npm run format` formats this directory;
`npm run format:repo` formats the repository's supported text files.

Use the root build commands for production bundles. They supply `BROWSER_BUNDLE` and regenerate
settings and API contracts before the browser build. If a Playwright update needs a new browser, run `just setup-browser`.

See [the frontend ownership map](src/lib/README.md) for components, state, and stories.
