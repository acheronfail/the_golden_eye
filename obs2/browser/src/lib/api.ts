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

/** Replay-buffer status reported by the backend. `enabled` reflects the OBS
 * profile setting (the recorder needs it on to save clips); `active` whether it
 * is currently running. Mirrors the Rust `ReplayBufferStatus`. */
export interface ReplayBufferStatus {
	enabled: boolean;
	active: boolean;
}

/** Fetch whether OBS's replay buffer is enabled (and running). Throws on a
 * non-OK response. */
export const getReplayBufferStatus = async (): Promise<ReplayBufferStatus> => {
	const res = await fetch(apiUrl('/api/v1/replay-buffer/status'));
	if (!res.ok) throw new Error(`Request error: ${res.status} ${await res.text()}`);
	return res.json();
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

/** Details of a clip the backend saved out of the replay buffer at the end of a
 * run. Mirrors the Rust `RecordingSaved`. */
export interface RecordingSaved {
	/** Absolute path to the trimmed clip written for the run. */
	path: string;
	/** The full replay-buffer file OBS saved, before trimming. */
	replayPath: string;
	/** Length of the trimmed clip, in seconds. */
	durationSecs: number;
	/** Whether a failure screen was seen during the run. */
	failed: boolean;
	/** The stats-screen match the clip was named from, when one was seen. */
	stats?: LevelMatch;
}

/** A message pushed over the monitor WebSocket. Mirrors the Rust `MonitorEvent`,
 * which is serialized internally tagged by `type`, so each variant is its
 * payload plus a discriminating `type` field. */
export type MonitorEvent =
	| ({ type: 'match' } & LevelMatch)
	| ({ type: 'recordingSaved' } & RecordingSaved);

/** Handlers for the messages the monitor WebSocket can push. All are optional;
 * provide only the ones you care about. */
export interface MonitorSocketHandlers {
	/** The matched on-screen state changed (also fired once on connect with the
	 * current match, if a monitor is running). */
	onMatch?: (match: LevelMatch) => void;
	/** A run's clip was saved out of the replay buffer and trimmed. */
	onRecordingSaved?: (saved: RecordingSaved) => void;
	/** Fires when the socket closes. */
	onClose?: () => void;
}

/**
 * Open a WebSocket to the backend that pushes {@link MonitorEvent} messages: the
 * latest {@link LevelMatch} whenever the matched state changes (and once on
 * connect), plus one-off events such as a recording being saved. Dispatches each
 * message to the matching handler. Returns the socket so callers can close it.
 */
export const connectMonitorSocket = (handlers: MonitorSocketHandlers): WebSocket => {
	const socket = new WebSocket(wsUrl('/api/v1/monitor/ws'));
	socket.onmessage = (event) => {
		const msg = JSON.parse(event.data) as MonitorEvent;
		switch (msg.type) {
			case 'match':
				handlers.onMatch?.(msg);
				break;
			case 'recordingSaved':
				handlers.onRecordingSaved?.(msg);
				break;
		}
	};
	if (handlers.onClose) socket.onclose = handlers.onClose;
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
