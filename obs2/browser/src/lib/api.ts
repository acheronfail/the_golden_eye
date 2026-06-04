// While developing, the SPA is served by the Vite dev server on its own port
// (see the `dev` just recipe) while the plugin's HTTP API lives on port 1337.
// Point API calls at that absolute origin in dev. In a production build the
// plugin serves the SPA itself, so relative URLs keep us origin-agnostic.
const API_ORIGIN = import.meta.env.DEV ? 'http://localhost:1337' : '';

/** Resolve an API path to a full URL appropriate for the current build mode. */
export const apiUrl = (path: string): string => `${API_ORIGIN}${path}`;
