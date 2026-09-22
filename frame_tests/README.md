# Frame regression tests

This harness runs the Rust `test_match` CLI against PNG fixtures. It derives expected results from
filenames and writes `test_results.json`. The Rust tests also use fixtures under `clips/` and
`screenshots-*`.

Run from the repository root:

```sh
just test-cv            # build the release matcher and run every frame case
just test-cv flicker    # filter fixture filenames with a regex
just bench-cv          # benchmark unique frame scenarios
```

After a release build, run `npm run test-watch` in this directory to repeat tests when the harness
changes. This command does not rebuild Rust. Rebuild with `just make-release` after matcher changes.

Add PNG fixtures to the appropriate `screenshots-*` capture-source directory. See `screenshots.ts`
for filename parsing and `frames.test.ts` for assertions. Keep filenames descriptive because they
serve as test expectations.

See [CONTRIBUTING.md](../CONTRIBUTING.md) for setup and the other test suites.
