// While developing, the SPA is served by the Vite dev server on its own port
// (see the `dev` just recipe) while the plugin's HTTP API lives on port 31337.
// Point API calls at that absolute origin in dev. In a production build the
// plugin serves the SPA itself, so relative URLs keep us origin-agnostic.
const API_ORIGIN = import.meta.env.DEV ? 'http://localhost:31337' : '';

/** Resolve an API path to a full URL appropriate for the current build mode. */
export const apiUrl = (path: string): string => `${API_ORIGIN}${path}`;

/**
 * Resolve an API path to a WebSocket URL. In dev we connect to the plugin's
 * absolute origin (port 31337); in a production build the SPA is served by the
 * plugin, so we derive the ws:// origin from the current page location.
 */
export const wsUrl = (path: string): string => {
	if (import.meta.env.DEV) return `ws://localhost:31337${path}`;
	const proto = window.location.protocol === 'https:' ? 'wss:' : 'ws:';
	return `${proto}//${window.location.host}${path}`;
};
