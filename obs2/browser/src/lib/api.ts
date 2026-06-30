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

/**
 * URL for a single-frame screenshot of the given OBS source, usable directly as
 * an `<img src>`. `lang` only influences the backend's (logged) match attempt,
 * not the returned image, so any valid language works when capturing a preview.
 */
export const screenshotUrl = (source: string): string =>
	apiUrl(`/api/v1/screenshot?source=${encodeURIComponent(source)}`);

export interface ObsSource {
	name: string;
	id: string;
}
export const getSources = async (): Promise<ObsSource[]> => {
	const res = await fetch(apiUrl('/api/v1/sources'));
	const data = await res.json();
	return data;
};

/** The level match the backend pushes over the monitor WebSocket. Mirrors
 * the Rust `LevelMatch` struct (`runtime_ms` is included but the backend
 * only pushes a new message when the rest of the state changes). */
export interface LevelMatch {
	screen: string;
	mission: number;
	part: number;
	difficulty: number;
	times: number[];
	runtime_ms: number;
}

/**
 * Open a WebSocket to the backend that pushes the latest {@link LevelMatch} (as
 * JSON) whenever the matched state changes. `onMatch` is invoked for each
 * message; `onClose` (optional) fires when the socket closes. Returns the socket
 * so callers can close it.
 */
export const connectMonitorSocket = (
	onMatch: (match: LevelMatch) => void,
	onClose?: () => void
): WebSocket => {
	const socket = new WebSocket(wsUrl('/api/v1/monitor/ws'));
	socket.onmessage = (event) => {
		onMatch(JSON.parse(event.data) as LevelMatch);
	};
	if (onClose) socket.onclose = onClose;
	return socket;
};

/** Current monitor status reported by the backend. `sourceName`/`lang` are only
 * present when `enabled` is true. Mirrors the Rust `MonitorStatus`. */
export type MonitorStatus =
	| { enabled: false }
	| { enabled: true; sourceName: string; lang: string };

/** Fetch whether a monitor is running, and if so for which source/language.
 * Throws on a non-OK response. */
export const getMonitorStatus = async (): Promise<MonitorStatus> => {
	const res = await fetch(apiUrl('/api/v1/monitor/status'));
	if (!res.ok) throw new Error(`Request error: ${res.status} ${await res.text()}`);
	return res.json();
};

/** Start monitoring the given source. Throws on a non-OK response. */
export const startMonitor = async (sourceName: string, lang: string): Promise<void> => {
	const res = await fetch(apiUrl('/api/v1/monitor/start'), {
		method: 'POST',
		headers: { 'content-type': 'application/json' },
		body: JSON.stringify({ sourceName, lang })
	});
	if (!res.ok) throw new Error(`Request error: ${res.status} ${await res.text()}`);
};

/** Stop monitoring. Throws on a non-OK response. */
export const stopMonitor = async (): Promise<void> => {
	const res = await fetch(apiUrl('/api/v1/monitor/stop'), {
		method: 'POST',
		headers: { 'content-type': 'application/json' }
	});
	if (!res.ok) throw new Error(`Request error: ${res.status} ${await res.text()}`);
};
