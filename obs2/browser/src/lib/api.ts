import type { Settings } from './settings.svelte';

// In dev the SPA is Vite-served on its own port while the API lives on 31337, so
// point API calls at that absolute origin. Production serves the SPA itself, so
// relative URLs stay origin-agnostic.
const API_ORIGIN = import.meta.env.DEV ? 'http://localhost:31337' : '';

/** Resolve an API path to a full URL appropriate for the current build mode. */
export const apiUrl = (path: string): string => `${API_ORIGIN}${path}`;

/** Resolve an API path to a WebSocket URL: the plugin's absolute origin (31337)
 * in dev, or the current page's origin in a production build. */
export const wsUrl = (path: string): string => {
	if (import.meta.env.DEV) return `ws://localhost:31337${path}`;
	const proto = window.location.protocol === 'https:' ? 'wss:' : 'ws:';
	return `${proto}//${window.location.host}${path}`;
};

/** URL for a single-frame screenshot of the given OBS source, usable as `<img src>`. */
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

/** Replay-buffer status: `enabled` (profile checkbox), `available`, `active`,
 * `maxSeconds` (window), and `outputDirectory` (replay files; default clip paths
 * derive from it). Mirrors Rust `ReplayBufferStatus`. */
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
	defaults: Settings;
	configPath: string;
	pluginVersion: string;
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

/** Applies whatever update is currently staged. Throws with a message
 * suitable for display when nothing is staged yet (404) or it's not
 * currently safe to apply one (409, e.g. a monitor session is active). */
export const applyUpdateNow = async (): Promise<void> => {
	const res = await fetch(apiUrl('/api/v1/updates/apply'), { method: 'POST' });
	if (res.status === 404) throw new Error('No update is staged yet -- try again in a moment.');
	if (res.status === 409) throw new Error('Cannot apply an update while monitoring or recording is active.');
	if (!res.ok) throw new Error(`Request error: ${res.status} ${await res.text()}`);
};

/** Checks for an update now, bypassing the configured check interval. Staging, if
 * one is found, happens in the background -- poll {@link getUpdateStatus} to see
 * when it's ready to apply. */
export const checkForUpdateNow = async (): Promise<{ update: PluginUpdate | null }> => {
	const res = await fetch(apiUrl('/api/v1/updates/check'), { method: 'POST' });
	if (!res.ok) throw new Error(`Request error: ${res.status} ${await res.text()}`);
	return res.json();
};

/** Downloads, verifies, and stages the latest release, resolving once ready to
 * apply. Explicit-download path, so it ignores the auto-update setting. Throws a
 * displayable message when already up to date (404). Follow with {@link applyUpdateNow}. */
export const downloadUpdateNow = async (): Promise<void> => {
	const res = await fetch(apiUrl('/api/v1/updates/download'), { method: 'POST' });
	if (res.status === 404) throw new Error('No newer release is available to download.');
	if (!res.ok) throw new Error(`Request error: ${res.status} ${await res.text()}`);
};

/** Whether a verified update is currently staged and ready to apply. */
export const getUpdateStatus = async (): Promise<{ staged: boolean }> => {
	const res = await fetch(apiUrl('/api/v1/updates/status'));
	if (!res.ok) throw new Error(`Request error: ${res.status} ${await res.text()}`);
	return res.json();
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
	/** Stats-screen times split into run / target / best; `null` on screens with
	 * no timed rows. `target_time` set only on the target's difficulty; `best_time`
	 * only once a prior time exists. */
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

/** Matches an image file (png/bmp) uploaded in the request body, for the
 * developer frame inspector. Coordinates are in the uploaded image's pixel space. */
export const matchUpload = async (
	file: Blob,
	lang: 'en' | 'jp',
	options: { annotations?: boolean } = {}
): Promise<MatchSourceResponse> => {
	const params = new URLSearchParams({
		lang,
		annotations: options.annotations ? 'true' : 'false'
	});
	const res = await fetch(apiUrl(`/api/v1/match/upload?${params.toString()}`), {
		method: 'POST',
		body: file
	});
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

/** Toggles the transient developer frame dump for `source`, which captures that
 * source's frames to a temp directory on disk independent of the monitor (the
 * backend logs the directory when dumping starts). `source` may be null when
 * disabling. */
export const setMonitorFrameDump = async (
	enabled: boolean,
	source: string | null,
	options: { signal?: AbortSignal; keepalive?: boolean } = {}
): Promise<boolean> => {
	const res = await fetch(apiUrl('/api/v1/monitor/frame-dump'), {
		method: 'POST',
		headers: { 'content-type': 'application/json' },
		body: JSON.stringify({ enabled, source }),
		signal: options.signal,
		keepalive: options.keepalive
	});
	if (!res.ok) throw new Error(`Request error: ${res.status} ${await res.text()}`);
	const data = (await res.json()) as { frameDumpEnabled: boolean };
	return data.frameDumpEnabled;
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

/** A scheduled save that was dropped before any clip was written (e.g. a failed
 * run shorter than the configured minimum), so no `RecordingSaved` follows.
 * Mirrors the Rust `MonitorEvent::RecordingSaveDiscarded`. */
export interface RecordingSaveDiscarded {
	/** Identifier of the pending save that was discarded. */
	saveId: number;
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

/** A transition in the recorder's per-run state. Mirrors Rust `RecordingStatus` */
export type RecordingStatus =
	| 'started'
	| 'cancelled'
	| 'failed'
	| 'aborted'
	| 'kia'
	| 'complete'
	| 'statsSkipped'
	| 'savePending';

/** Why a failed run reached an ending screen without a clip being saved.
 * Mirrors the Rust `FailedRunNotSavedReason`. */
export type FailedRunNotSavedReason = 'savingDisabled' | 'tooShort';

/** Why the backend stopped monitoring. Mirrors the Rust `MonitorStoppedReason`. */
export type MonitorStoppedReason = 'userStopped' | 'replayBufferStopped';

export interface MonitorFps {
	processedFps: number;
	sourceFps: number;
}

/** A message pushed over the app WebSocket. Mirrors the Rust `MonitorEvent`,
 * which is serialized internally tagged by `type`, so each variant is its
 * payload plus a discriminating `type` field. */
export interface MonitorSnapshot {
	enabled: boolean;
	sourceName?: string;
}

export interface AppSnapshot {
	monitor: MonitorSnapshot;
	match: LevelMatch | null;
	recordingState: RecordingStatus | null;
	sources: ObsSource[];
	replayBuffer: ReplayBufferStatus;
	settingsStatus: SettingsStatus;
	update: PluginUpdate | null;
}

export type AppSocketEvent =
	| { type: 'version'; buildId: string }
	| { type: 'snapshot'; state: AppSnapshot }
	| { type: 'languageDetected'; lang: 'en' | 'jp' }
	| ({ type: 'monitorFps' } & MonitorFps)
	| ({ type: 'recordingSavePending' } & RecordingSavePending)
	| ({ type: 'recordingSaved' } & RecordingSaved)
	| ({ type: 'recordingSaveDiscarded' } & RecordingSaveDiscarded)
	| { type: 'failedRunNotSaved'; reason: FailedRunNotSavedReason }
	| { type: 'monitorStopped'; reason: MonitorStoppedReason }
	| { type: 'settingsReloaded'; configPath: string; settings: Settings }
	| { type: 'settingsInvalid'; configPath: string; error: string }
	| { type: 'updateApplied'; version: string; releaseUrl?: string }
	| { type: 'updateStagingFailed'; error: string };

/** Handlers for the messages the app WebSocket can push. All are optional;
 * provide only the ones you care about. */
export interface AppSocketHandlers {
	/** Complete retained app/session state from the backend. */
	onSnapshot?: (snapshot: AppSnapshot) => void;
	/** The active source showed a ROM language marker. */
	onLanguageDetected?: (lang: 'en' | 'jp') => void;
	/** Periodic monitor throughput telemetry while a monitor is active. */
	onMonitorFps?: (fps: MonitorFps) => void;
	/** A run's clip save was scheduled after the post-run padding. */
	onRecordingSavePending?: (pending: RecordingSavePending) => void;
	/** A run's clip was saved out of the replay buffer and trimmed. */
	onRecordingSaved?: (saved: RecordingSaved) => void;
	/** A scheduled save was discarded before writing a clip, so any pending-save
	 * notification for it should be cleared. */
	onRecordingSaveDiscarded?: (discarded: RecordingSaveDiscarded) => void;
	/** A failed run reached an ending but no clip was saved (saving disabled, or
	 * the run was too short). Surfaced as a transient notification. */
	onFailedRunNotSaved?: (reason: FailedRunNotSavedReason) => void;
	/** Monitoring stopped in the backend. */
	onMonitorStopped?: (reason: MonitorStoppedReason) => void;
	/** Settings JSON was reloaded from disk. */
	onSettingsReloaded?: (settings: Settings, configPath: string) => void;
	/** Settings JSON changed but is invalid. */
	onSettingsInvalid?: (error: string, configPath: string) => void;
	/** The plugin just applied an update and is now running `version`. Fires once
	 * per applied update. `releaseUrl` is present only when it's confidently the
	 * changelog for the update just applied. */
	onUpdateApplied?: (version: string, releaseUrl?: string) => void;
	/** A newer release was found but downloading/verifying/staging it failed, so
	 * no update is queued up to apply. */
	onUpdateStagingFailed?: (error: string) => void;
	/** Fires when the socket closes. */
	onClose?: () => void;
}

/** Build id this page was served with, from the `<meta>` tag the backend injects.
 * `null` in dev (Vite-served, no injection), where the version check is skipped. */
export const selfBuildId = (): string | null =>
	document.querySelector('meta[name="ge-build-id"]')?.getAttribute('content') ?? null;

/** Reload if the backend serves a different frontend build than this tab, e.g. a
 * stale cached page left open across an update. Entry HTML is `no-store`, so the
 * reload lands on the current build. */
const reloadIfStale = (backendBuildId: string): void => {
	const self = selfBuildId();
	// No meta tag means dev mode (Vite-served SPA); nothing to compare against.
	if (self !== null && self !== backendBuildId) {
		console.warn(`frontend build ${self} differs from backend build ${backendBuildId}; reloading`);
		window.location.reload();
	}
};

/** Open a WebSocket that pushes {@link AppSocketEvent} messages (version
 * handshake, sources, matches, recorder transitions, save events), dispatching
 * each to the matching handler. Returns the socket so callers can close it. */
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
			case 'snapshot':
				if (msg.state && typeof msg.state === 'object') {
					handlers.onSnapshot?.(msg.state);
				} else {
					console.warn('Ignoring malformed snapshot event', msg);
				}
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
			case 'recordingSaveDiscarded':
				handlers.onRecordingSaveDiscarded?.(msg);
				break;
			case 'failedRunNotSaved':
				handlers.onFailedRunNotSaved?.(msg.reason);
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
			case 'updateApplied':
				if (typeof msg.version === 'string') {
					handlers.onUpdateApplied?.(msg.version, msg.releaseUrl);
				} else {
					console.warn('Ignoring malformed updateApplied event', msg);
				}
				break;
			case 'updateStagingFailed':
				handlers.onUpdateStagingFailed?.(msg.error);
				break;
		}
	};
	if (handlers.onClose) socket.onclose = handlers.onClose;
	return socket;
};

export type MonitorStatus =
	| { enabled: false; recordingState?: null }
	| { enabled: true; sourceName: string; recordingState: RecordingStatus | null };

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
