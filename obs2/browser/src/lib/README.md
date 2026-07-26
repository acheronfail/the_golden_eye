# Frontend library structure

Organize code by ownership first. Keep related components, reactive controllers, models, pure
calculations, and tests together instead of separating them into catch-all technical directories.

## Directories

- `app/` contains root-layout and application-shell UI.
- `features/<name>/` contains code owned by one product capability.
- `ui/` contains domain-independent components reused across features.
- `stores/` contains reactive state shared by multiple features.
- `developer/` contains developer-only UI and logic.
- `test/`, `assets/`, and `api.ts` contain shared infrastructure, static assets, and API contracts.

Feature directories stay flat by default. Give a complex component its own PascalCase directory only
when it has meaningful private children or implementation modules, as `ui/Chart/` and
`features/monitor/KiaDeathOverlay/` do.

## Files and imports

- Keep component-private props and types in `Thing.svelte`.
- Use `thing.svelte.ts` for rune-based reactive state or controllers.
- Use a specific name such as `renderer.ts`, `geometry.ts`, or `runMetadata.ts` for pure logic and
  shared feature models.
- Put `*.spec.ts` beside the file it tests.
- Avoid generic `components/`, `controllers/`, `effects/`, `types/`, `utils/`, and `helpers/`
  directories.
- Prefer relative imports within the same feature or component capsule and `$lib/...` imports across
  ownership boundaries.
- Do not add barrel files by default. Add a feature entry point only when it enforces a useful public
  boundary.

Storybook files live under `src/stories/` and mirror the source owner: `app/`, `ui/`,
`features/<name>/`, or `developer/`. Shared story fixtures may remain at the stories root.
