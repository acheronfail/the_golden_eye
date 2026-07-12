import type { Settings } from './settings.svelte';

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
 * an `<img src>`.
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

export interface ClipMetadata {
	timestamp: string;
	time?: string;
	timeSeconds?: number;
	level: string;
	levelNumber?: number;
	difficulty?: string;
	status: string;
	romLanguage: string;
	sourceName: string;
	comment: string;
	pluginVersion: string;
}

export interface RunDirectoryScan {
	kind: 'completed' | 'failed';
	path: string;
	exists: boolean;
	error?: string | null;
}

export interface RunClip {
	path: string;
	fileName: string;
	directory: string;
	sizeBytes: number;
	modified?: string | null;
	durationSecs?: number | null;
	metadata: ClipMetadata;
}

export interface RunsResponse {
	directories: RunDirectoryScan[];
	clips: RunClip[];
}

export type RunsStreamEvent =
	| { type: 'directory'; directory: RunDirectoryScan }
	| { type: 'clip'; clip: RunClip }
	| { type: 'done' };

export const getRuns = async (): Promise<RunsResponse> => {
	const res = await fetch(apiUrl('/api/v1/runs'));
	if (!res.ok) throw new Error(`Request error: ${res.status} ${await res.text()}`);
	return res.json();
};

export const streamRuns = async (onEvent: (event: RunsStreamEvent) => void, signal?: AbortSignal): Promise<void> => {
	const res = await fetch(apiUrl('/api/v1/runs/stream'), { signal });
	if (!res.ok) throw new Error(`Request error: ${res.status} ${await res.text()}`);
	if (!res.body) {
		const runs = await getRuns();
		for (const directory of runs.directories) onEvent({ type: 'directory', directory });
		for (const clip of runs.clips) onEvent({ type: 'clip', clip });
		onEvent({ type: 'done' });
		return;
	}

	const reader = res.body.getReader();
	const decoder = new TextDecoder();
	let buffer = '';

	while (true) {
		const { value, done } = await reader.read();
		buffer += decoder.decode(value, { stream: !done });
		const lines = buffer.split('\n');
		buffer = lines.pop() ?? '';
		for (const line of lines) {
			if (line.trim()) onEvent(JSON.parse(line) as RunsStreamEvent);
		}
		if (done) break;
	}

	if (buffer.trim()) onEvent(JSON.parse(buffer) as RunsStreamEvent);
};

export const runThumbnailUrl = (path: string): string =>
	apiUrl(`/api/v1/runs/thumbnail?path=${encodeURIComponent(path)}`);

export const runVideoUrl = (path: string): string => apiUrl(`/api/v1/runs/video?path=${encodeURIComponent(path)}`);

export interface EditableRunMetadata {
	romLanguage: string;
	status: string;
	difficulty: string;
	time: string;
	level: string;
}

export const deleteRun = async (path: string): Promise<void> => {
	const res = await fetch(apiUrl(`/api/v1/runs?path=${encodeURIComponent(path)}`), { method: 'DELETE' });
	if (!res.ok) throw new Error(`Request error: ${res.status} ${await res.text()}`);
};

type FileRevealRequest =
	| { target: 'run'; path: string }
	| { target: 'runFolder'; kind: RunDirectoryScan['kind'] }
	| { target: 'settingsConfig' };

export const revealFile = async (request: FileRevealRequest): Promise<void> => {
	const res = await fetch(apiUrl('/api/v1/files/reveal'), {
		method: 'POST',
		headers: { 'content-type': 'application/json' },
		body: JSON.stringify(request)
	});
	if (!res.ok) throw new Error(`Request error: ${res.status} ${await res.text()}`);
};

export const revealRun = async (path: string): Promise<void> => revealFile({ target: 'run', path });

export const revealRunFolder = async (kind: RunDirectoryScan['kind']): Promise<void> =>
	revealFile({ target: 'runFolder', kind });

export const renameRun = async (path: string, fileName: string): Promise<RunClip> => {
	const res = await fetch(apiUrl('/api/v1/runs/rename'), {
		method: 'POST',
		headers: { 'content-type': 'application/json' },
		body: JSON.stringify({ path, fileName })
	});
	if (!res.ok) throw new Error(`Request error: ${res.status} ${await res.text()}`);
	return res.json();
};

export const updateRunMetadata = async (path: string, metadata: EditableRunMetadata): Promise<RunClip> => {
	const res = await fetch(apiUrl('/api/v1/runs'), {
		method: 'PATCH',
		headers: { 'content-type': 'application/json' },
		body: JSON.stringify({ path, metadata })
	});
	if (!res.ok) throw new Error(`Request error: ${res.status} ${await res.text()}`);
	return res.json();
};

/** Replay-buffer status reported by the backend. `enabled` reflects the OBS
 * profile checkbox; `available` whether OBS has a replay-buffer output for the
 * current output settings; `active` whether it is currently running; and
 * `maxSeconds` the configured replay-buffer window. `outputDirectory` is the
 * OBS directory replay files are written into; default clip paths are derived
 * from it. Mirrors the Rust
 * `ReplayBufferStatus`. */
export interface ReplayBufferStatus {
	enabled: boolean;
	available: boolean;
	active: boolean;
	maxSeconds: number | null;
	outputDirectory: string | null;
	defaultCompletedOutputPath: string | null;
	defaultFailedOutputPath: string | null;
}

/** Fetch whether OBS's replay buffer is enabled/available (and running). Throws
 * on a non-OK response. */
export const getReplayBufferStatus = async (): Promise<ReplayBufferStatus> => {
	const res = await fetch(apiUrl('/api/v1/replay-buffer/status'));
	if (!res.ok) throw new Error(`Request error: ${res.status} ${await res.text()}`);
	return res.json();
};

/** Fetch the plugin-owned settings JSON. Throws on a non-OK response. */
export const getSettings = async (): Promise<Settings> => {
	const res = await fetch(apiUrl('/api/v1/settings'));
	if (!res.ok) throw new Error(`Request error: ${res.status} ${await res.text()}`);
	return res.json();
};

export interface SettingsStatus {
	settings: Settings;
	configPath: string;
	fileError?: string | null;
}

/** Fetch settings plus the on-disk config status. Throws on a non-OK response. */
export const getSettingsStatus = async (): Promise<SettingsStatus> => {
	const res = await fetch(apiUrl('/api/v1/settings/status'));
	if (!res.ok) throw new Error(`Request error: ${res.status} ${await res.text()}`);
	return res.json();
};

/** Persist the complete settings object through the Rust backend. */
export const putSettings = async (settings: Settings): Promise<Settings> => {
	const res = await fetch(apiUrl('/api/v1/settings'), {
		method: 'PUT',
		headers: { 'content-type': 'application/json' },
		body: JSON.stringify(settings)
	});
	if (!res.ok) throw new Error(`Request error: ${res.status} ${await res.text()}`);
	return res.json();
};

export const resetSettingsToDefaults = async (): Promise<Settings> => {
	const res = await fetch(apiUrl('/api/v1/settings/reset'), { method: 'POST' });
	if (!res.ok) throw new Error(`Request error: ${res.status} ${await res.text()}`);
	return res.json();
};

export const revealSettingsConfig = async (): Promise<void> => revealFile({ target: 'settingsConfig' });

export interface PluginUpdate {
	currentVersion: string;
	latestVersion: string;
	releaseUrl: string;
}

export const openUpdateRelease = async (releaseUrl: string): Promise<void> => {
	const res = await fetch(apiUrl('/api/v1/updates/open'), {
		method: 'POST',
		headers: { 'content-type': 'application/json' },
		body: JSON.stringify({ releaseUrl })
	});
	if (!res.ok) throw new Error(`Request error: ${res.status} ${await res.text()}`);
};

export interface FolderPickResult {
	cancelled: boolean;
	path?: string | null;
}

export interface FolderValidation {
	expandedPath: string;
	empty: boolean;
	exists: boolean;
	isDirectory: boolean;
	writable: boolean;
	willCreate: boolean;
	error?: string | null;
}

/** Open the plugin backend's native folder picker. The browser never receives a
 * `FileSystemDirectoryHandle`; it gets the OS path Rust needs for clip output. */
export const pickFolder = async (options: { title: string; currentPath?: string }): Promise<FolderPickResult> => {
	const res = await fetch(apiUrl('/api/v1/folders/pick'), {
		method: 'POST',
		headers: { 'content-type': 'application/json' },
		body: JSON.stringify(options)
	});
	if (!res.ok) throw new Error(`Request error: ${res.status} ${await res.text()}`);
	return res.json();
};

/** Validate a folder path from the same process that will later write clips. */
export const validateFolder = async (path: string): Promise<FolderValidation> => {
	const res = await fetch(apiUrl('/api/v1/folders/validate'), {
		method: 'POST',
		headers: { 'content-type': 'application/json' },
		body: JSON.stringify({ path })
	});
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
	/** ROM language detected from language-specific static UI, when visible. */
	detected_lang?: 'en' | 'jp';
	/** The stats-screen times split into run / target / best. `null` on any
	 * screen with no timed rows (start, report screens, gameplay). `target_time`
	 * is set only when the level was completed on the difficulty its target is
	 * defined for; `best_time` only once a prior time exists. */
	times: {
		time: number;
		target_time: number | null;
		best_time: number | null;
	} | null;
	raw_times?: number[];
	match_regions?: {
		label: string;
		x: number;
		y: number;
		w: number;
		h: number;
		score: number;
	}[];
	annotation_sets?: AnnotationSet[];
	runtime_ms: number;
}

export interface AnnotationRect {
	label: string;
	x: number;
	y: number;
	w: number;
	h: number;
	score?: number;
}

export interface AnnotationSet {
	id: string;
	label: string;
	annotations: AnnotationRect[];
}

export interface MatchSourceResponse {
	match: LevelMatch;
	annotationsEnabled: boolean;
	frameWidth: number;
	frameHeight: number;
}

export const matchSource = async (
	source: string,
	lang: 'en' | 'jp',
	options: { annotations?: boolean } = {}
): Promise<MatchSourceResponse> => {
	const params = new URLSearchParams({
		source,
		lang,
		annotations: options.annotations ? 'true' : 'false'
	});
	const res = await fetch(apiUrl(`/api/v1/match?${params.toString()}`), { method: 'POST' });
	if (!res.ok) throw new Error(`Request error: ${res.status} ${await res.text()}`);
	return res.json();
};

export const setMonitorMatcherAnnotations = async (
	annotations: boolean,
	options: { signal?: AbortSignal; keepalive?: boolean } = {}
): Promise<boolean> => {
	const res = await fetch(apiUrl('/api/v1/match/annotations'), {
		method: 'POST',
		headers: { 'content-type': 'application/json' },
		body: JSON.stringify({ annotations }),
		signal: options.signal,
		keepalive: options.keepalive
	});
	if (!res.ok) throw new Error(`Request error: ${res.status} ${await res.text()}`);
	const data = (await res.json()) as { annotationsEnabled: boolean };
	return data.annotationsEnabled;
};

/** Details of a clip the backend saved out of the replay buffer at the end of a
 * run. Mirrors the Rust `RecordingSaved`. */
export interface RecordingSaved {
	/** Identifier shared with the matching pending-save event. */
	saveId: number;
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

/** Details of a clip save that has been scheduled after a run ending was seen.
 * Mirrors the Rust `RecordingSavePending`. */
export interface RecordingSavePending {
	/** Identifier shared with the matching saved event. */
	saveId: number;
	/** Seconds until OBS replay-buffer save is requested. */
	saveInSecs: number;
	/** Expected trimmed clip length, before replay-buffer duration clamping. */
	estimatedDurationSecs: number;
	/** Whether a failure screen was seen during the run. */
	failed: boolean;
	/** Final run status used for naming/metadata. */
	status: string;
	/** Human-readable level name, or "unknown" if unavailable. */
	level: string;
	/** GoldenEye campaign level number, when known. */
	levelNumber?: number;
	/** Human-readable difficulty label, when known. */
	difficulty?: string;
	/** Run time read from the stats screen, in seconds, when known. */
	timeSecs?: number;
	/** Target time read from the stats screen, in seconds, when present. */
	targetTimeSecs?: number;
	/** Best time read from the stats screen, in seconds, when present. */
	bestTimeSecs?: number;
	/** The stats-screen match the clip will be named from, when one was seen. */
	stats?: LevelMatch;
}

/** Recording configuration stored by the Rust backend and mirrored in the local
 * `$lib` settings store. Mirrors the Rust `RecordingOptions`. */
export interface RecordingOptions {
	completedOutputPath: string;
	saveFailedRuns: boolean;
	failedOutputPath: string;
	failedRunLimit: number;
	minimumFailedRunLengthSecs: number;
	clipFilenameTemplate: string;
	preRunPaddingSecs: number;
	postRunPaddingSecs: number;
}

/** A transition in the backend recorder's per-run state. Mirrors the Rust
 * `RecordingStatus`:
 * - `started` — a run began (the clip's start was anchored);
 * - `cancelled` — the active run was abandoned mid-play before reaching its
 *   report screen (back to the level grid), so nothing is saved for it;
 * - `failed` / `aborted` / `kia` — a failure report screen was seen during the
 *   run (mission failed / mission aborted / killed in action); the specific one
 *   names why the run ended. The clip is still saved;
 * - `complete` — the mission-complete report screen was reached; the run is a
 *   success (sent once per run, and also clears a prior failure);
 * - `statsSkipped` — a *completed* run's stats screen was bypassed (the user
 *   backed out of the report screen to the level grid); the clip is still saved
 *   (a {@link RecordingSaved} still follows). A failed run backing out this way
 *   emits `savePending` instead;
 * - `failedDiscarded` — a failed run ended, but failed-run saving is disabled
 *   or the stats-screen run time, falling back to detected duration, is shorter
 *   than the configured minimum failed-run length;
 * - `savePending` — a run ended (at the stats screen, or a failed run backing
 *   out) and a save was scheduled; a {@link RecordingSaved} follows once the clip
 *   is written. */
export type RecordingStatus =
	| 'started'
	| 'cancelled'
	| 'failed'
	| 'aborted'
	| 'kia'
	| 'complete'
	| 'statsSkipped'
	| 'failedDiscarded'
	| 'savePending';

/** Why the backend stopped monitoring. Mirrors the Rust `MonitorStoppedReason`. */
export type MonitorStoppedReason = 'userStopped' | 'replayBufferStopped';

export interface MonitorFps {
	processedFps: number;
	sourceFps: number;
}

/** A message pushed over the app WebSocket. Mirrors the Rust `MonitorEvent`,
 * which is serialized internally tagged by `type`, so each variant is its
 * payload plus a discriminating `type` field. */
export type AppSocketEvent =
	| { type: 'version'; buildId: string }
	| { type: 'sources'; sources: ObsSource[] }
	| ({ type: 'match' } & LevelMatch)
	| { type: 'recordingState'; status: RecordingStatus | null }
	| { type: 'languageDetected'; lang: 'en' | 'jp' }
	| ({ type: 'monitorFps' } & MonitorFps)
	| ({ type: 'recordingSavePending' } & RecordingSavePending)
	| ({ type: 'recordingSaved' } & RecordingSaved)
	| { type: 'monitorStopped'; reason: MonitorStoppedReason }
	| { type: 'settingsReloaded'; configPath: string; settings: Settings }
	| { type: 'settingsInvalid'; configPath: string; error: string }
	| ({ type: 'updateAvailable' } & PluginUpdate);

/** Handlers for the messages the app WebSocket can push. All are optional;
 * provide only the ones you care about. */
export interface AppSocketHandlers {
	/** The OBS renderable video-source list changed, or was replayed on connect. */
	onSources?: (sources: ObsSource[]) => void;
	/** The matched on-screen state changed (also fired once on connect with the
	 * current match, if a monitor is running). */
	onMatch?: (match: LevelMatch) => void;
	/** The recorder's per-run state changed, or returned to idle (`null`). */
	onRecordingState?: (status: RecordingStatus | null) => void;
	/** The active source showed a ROM language marker. */
	onLanguageDetected?: (lang: 'en' | 'jp') => void;
	/** Periodic monitor throughput telemetry while a monitor is active. */
	onMonitorFps?: (fps: MonitorFps) => void;
	/** A run's clip save was scheduled after the post-run padding. */
	onRecordingSavePending?: (pending: RecordingSavePending) => void;
	/** A run's clip was saved out of the replay buffer and trimmed. */
	onRecordingSaved?: (saved: RecordingSaved) => void;
	/** Monitoring stopped in the backend. */
	onMonitorStopped?: (reason: MonitorStoppedReason) => void;
	/** Settings JSON was reloaded from disk. */
	onSettingsReloaded?: (settings: Settings, configPath: string) => void;
	/** Settings JSON changed but is invalid. */
	onSettingsInvalid?: (error: string, configPath: string) => void;
	/** A newer plugin release is available. */
	onUpdateAvailable?: (update: PluginUpdate) => void;
	/** Fires when the socket closes. */
	onClose?: () => void;
}

/** The build id this page was served with, read from the `<meta>` tag the
 * backend injects into the SPA's HTML. `null` in dev, where the SPA is served by
 * Vite (no injection) — the version check is skipped there. */
const selfBuildId = (): string | null =>
	document.querySelector('meta[name="ge-build-id"]')?.getAttribute('content') ?? null;

/** Reload the page if the backend is serving a different frontend build than the
 * one this tab is running. Catches a stale tab — an older cached page, or one
 * left open across a plugin update — that reconnected to an updated backend. The
 * entry HTML is served `no-store`, so the reload lands on the current build. */
const reloadIfStale = (backendBuildId: string): void => {
	const self = selfBuildId();
	// No meta tag means dev mode (Vite-served SPA); nothing to compare against.
	if (self !== null && self !== backendBuildId) {
		console.warn(`frontend build ${self} differs from backend build ${backendBuildId}; reloading`);
		window.location.reload();
	}
};

/**
 * Open a WebSocket to the backend that pushes {@link AppSocketEvent} messages:
 * a one-off `version` handshake, the current OBS source list and subsequent
 * source changes, the latest {@link LevelMatch} whenever the matched state
 * changes (and once on connect), recorder state transitions, and one-off events
 * such as a recording being saved. Dispatches each message to the matching
 * handler. Returns the socket so callers can close it.
 */
export const connectAppSocket = (handlers: AppSocketHandlers): WebSocket => {
	const socket = new WebSocket(wsUrl('/api/v1/monitor/ws'));
	socket.onmessage = (event) => {
		const msg = JSON.parse(event.data) as AppSocketEvent;
		switch (msg.type) {
			case 'version':
				if (typeof msg.buildId === 'string') {
					reloadIfStale(msg.buildId);
				} else {
					console.warn('Ignoring malformed monitor version event', msg);
				}
				break;
			case 'sources':
				if (Array.isArray(msg.sources)) {
					handlers.onSources?.(msg.sources);
				} else {
					console.warn('Ignoring malformed sources event', msg);
				}
				break;
			case 'match':
				handlers.onMatch?.(msg);
				break;
			case 'recordingState':
				handlers.onRecordingState?.(msg.status);
				break;
			case 'languageDetected':
				handlers.onLanguageDetected?.(msg.lang);
				break;
			case 'monitorFps':
				handlers.onMonitorFps?.(msg);
				break;
			case 'recordingSavePending':
				handlers.onRecordingSavePending?.(msg);
				break;
			case 'recordingSaved':
				handlers.onRecordingSaved?.(msg);
				break;
			case 'monitorStopped':
				handlers.onMonitorStopped?.(msg.reason);
				break;
			case 'settingsReloaded':
				handlers.onSettingsReloaded?.(msg.settings, msg.configPath);
				break;
			case 'settingsInvalid':
				handlers.onSettingsInvalid?.(msg.error, msg.configPath);
				break;
			case 'updateAvailable':
				handlers.onUpdateAvailable?.(msg);
				break;
		}
	};
	if (handlers.onClose) socket.onclose = handlers.onClose;
	return socket;
};

/** Current monitor status reported by the backend. `sourceName` is only present
 * when `enabled` is true. `recordingState` is the backend-retained
 * recorder phase, or `null` when no run phase is active. Mirrors the Rust
 * `MonitorStatus`. */
export type MonitorStatus =
	| { enabled: false; recordingState?: null }
	| { enabled: true; sourceName: string; recordingState: RecordingStatus | null };

/** Fetch whether a monitor is running, and if so for which source.
 * Throws on a non-OK response. */
export const getMonitorStatus = async (): Promise<MonitorStatus> => {
	const res = await fetch(apiUrl('/api/v1/monitor/status'));
	if (!res.ok) throw new Error(`Request error: ${res.status} ${await res.text()}`);
	return res.json();
};

/** Start monitoring the given source. Recording options are read by the backend
 * from the persisted settings store. Throws on a non-OK response. */
export const startMonitor = async (sourceName: string): Promise<void> => {
	const res = await fetch(apiUrl('/api/v1/monitor/start'), {
		method: 'POST',
		headers: { 'content-type': 'application/json' },
		body: JSON.stringify({ sourceName })
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
