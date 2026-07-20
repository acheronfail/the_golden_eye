import type { Settings } from './settings.svelte';

// In dev the SPA is Vite-served on its own port while the API lives on 31337, so
// point API calls at that absolute origin. Production serves the SPA itself, so
// relative URLs stay origin-agnostic.
const API_ORIGIN = import.meta.env.DEV ? 'http://localhost:31337' : '';

type RequestErrorMessages = Partial<Record<number, string>>;

type FileRevealRequest =
	| { target: 'run'; path: string }
	| { target: 'runFolder'; kind: RunDirectoryScan['kind'] }
	| { target: 'settingsConfig' };

export class Backend {
	/** Resolve an API path to a full URL appropriate for the current build mode. */
	public apiUrl(path: string): string {
		return `${API_ORIGIN}${path}`;
	}

	/** Resolve an API path to a WebSocket URL. */
	public wsUrl(path: string): string {
		if (import.meta.env.DEV) return `ws://localhost:31337${path}`;
		const proto = window.location.protocol === 'https:' ? 'wss:' : 'ws:';
		return `${proto}//${window.location.host}${path}`;
	}

	/** URL for a single-frame screenshot of the given OBS source, usable as `<img src>`. */
	public screenshotUrl(source: string): string {
		return this.apiUrl(`/api/v1/screenshot?source=${encodeURIComponent(source)}`);
	}

	public getSources(): Promise<ObsSource[]> {
		return this.getJson('/api/v1/sources');
	}

	public getRuns(): Promise<RunsResponse> {
		return this.getJson('/api/v1/runs');
	}

	public async streamRuns(
		onEvent: (event: RunsStreamEvent) => void,
		signal?: AbortSignal,
		options: { refresh?: boolean } = {}
	): Promise<void> {
		const path = options.refresh ? '/api/v1/runs/stream?refresh=true' : '/api/v1/runs/stream';
		const res = await this.request(path, { signal });
		if (!res.body) {
			const runs = await this.getRuns();
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
	}

	public runVideoUrl(path: string): string {
		return this.apiUrl(`/api/v1/runs/video?path=${encodeURIComponent(path)}`);
	}

	public deleteRun(path: string): Promise<void> {
		return this.delete(`/api/v1/runs?path=${encodeURIComponent(path)}`);
	}

	public revealFile(request: FileRevealRequest): Promise<void> {
		return this.postJsonVoid('/api/v1/files/reveal', request);
	}

	public revealRun(path: string): Promise<void> {
		return this.revealFile({ target: 'run', path });
	}

	public revealRunFolder(kind: RunDirectoryScan['kind']): Promise<void> {
		return this.revealFile({ target: 'runFolder', kind });
	}

	public renameRun(path: string, fileName: string): Promise<RunClip> {
		return this.postJson('/api/v1/runs/rename', { path, fileName });
	}

	public getYouTubeStatus(): Promise<YouTubeStatus> {
		return this.getJson('/api/v1/youtube/status');
	}

	public connectYouTube(): Promise<YouTubeStatus> {
		return this.post('/api/v1/youtube/connect');
	}

	public disconnectYouTube(): Promise<YouTubeStatus> {
		return this.post('/api/v1/youtube/disconnect');
	}

	public uploadRunToYouTube(path: string, options?: YouTubeUploadOptions): Promise<YouTubeUploadStatus> {
		return this.postJson('/api/v1/youtube/upload', { path, ...options });
	}

	public openYouTubeUrl(url: string): Promise<void> {
		return this.postJsonVoid('/api/v1/youtube/open', { url });
	}

	public forgetYouTubeUpload(path: string): Promise<YouTubeStatus> {
		return this.postJson('/api/v1/youtube/forget', { path });
	}

	public updateRunMetadata(path: string, metadata: EditableRunMetadata): Promise<RunClip> {
		return this.patchJson('/api/v1/runs', { path, metadata });
	}

	/** Fetch whether OBS's replay buffer is enabled/available (and running). */
	public getReplayBufferStatus(): Promise<ReplayBufferStatus> {
		return this.getJson('/api/v1/replay-buffer/status');
	}

	/** Fetch the plugin-owned settings JSON. */
	public getSettings(): Promise<Settings> {
		return this.getJson('/api/v1/settings');
	}

	/** Fetch settings plus the on-disk config status. */
	public getSettingsStatus(): Promise<SettingsStatus> {
		return this.getJson('/api/v1/settings/status');
	}

	/** Persist the complete settings object through the Rust backend. */
	public putSettings(settings: Settings): Promise<Settings> {
		return this.putJson('/api/v1/settings', settings);
	}

	public resetSettingsToDefaults(): Promise<Settings> {
		return this.post('/api/v1/settings/reset');
	}

	public revealSettingsConfig(): Promise<void> {
		return this.revealFile({ target: 'settingsConfig' });
	}

	public openUpdateRelease(releaseUrl: string): Promise<void> {
		return this.postJsonVoid('/api/v1/updates/open', { releaseUrl });
	}

	/** Applies whatever update is currently staged. */
	public applyUpdateNow(): Promise<void> {
		return this.postVoid('/api/v1/updates/apply', {
			404: 'No update is staged yet -- try again in a moment.',
			409: 'Cannot apply an update while monitoring or recording is active.'
		});
	}

	/** Checks for an update now, bypassing the configured check interval. */
	public checkForUpdateNow(): Promise<{ update: PluginUpdate | null }> {
		return this.post('/api/v1/updates/check');
	}

	/** Downloads, verifies, and stages the latest release. */
	public downloadUpdateNow(): Promise<void> {
		return this.postVoid('/api/v1/updates/download', {
			404: 'No newer release is available to download.'
		});
	}

	/** Whether a verified update is currently staged and ready to apply. */
	public getUpdateStatus(): Promise<{ staged: boolean }> {
		return this.getJson('/api/v1/updates/status');
	}

	/** Open the plugin backend's native folder picker. */
	public pickFolder(options: { title: string; currentPath?: string }): Promise<FolderPickResult> {
		return this.postJson('/api/v1/folders/pick', options);
	}

	/** Validate a folder path from the same process that will later write clips. */
	public validateFolder(path: string): Promise<FolderValidation> {
		return this.postJson('/api/v1/folders/validate', { path });
	}

	public matchSource(
		source: string,
		lang: 'en' | 'jp',
		options: { annotations?: boolean } = {}
	): Promise<MatchSourceResponse> {
		const params = this.query({ source, lang, annotations: this.bool(options.annotations) });
		return this.post(`/api/v1/match?${params}`);
	}

	/** Matches an image file (png/bmp) uploaded in the request body. */
	public matchUpload(
		file: Blob,
		lang: 'en' | 'jp',
		options: { annotations?: boolean } = {}
	): Promise<MatchSourceResponse> {
		const params = this.query({ lang, annotations: this.bool(options.annotations) });
		return this.post(`/api/v1/match/upload?${params}`, { body: file });
	}

	public async setMonitorMatcherAnnotations(
		annotations: boolean,
		options: { signal?: AbortSignal; keepalive?: boolean } = {}
	): Promise<boolean> {
		const data = await this.postJson<{ annotationsEnabled: boolean }>(
			'/api/v1/match/annotations',
			{ annotations },
			options
		);
		return data.annotationsEnabled;
	}

	/** Toggles the transient developer frame dump for `source`. */
	public async setMonitorFrameDump(
		enabled: boolean,
		source: string | null,
		options: { signal?: AbortSignal; keepalive?: boolean } = {}
	): Promise<boolean> {
		const data = await this.postJson<{ frameDumpEnabled: boolean }>(
			'/api/v1/monitor/frame-dump',
			{ enabled, source },
			options
		);
		return data.frameDumpEnabled;
	}

	/** Build id this page was served with, from the injected `<meta>` tag. */
	public selfBuildId(): string | null {
		return document.querySelector('meta[name="ge-build-id"]')?.getAttribute('content') ?? null;
	}

	/** Open the app WebSocket and dispatch {@link AppSocketEvent} messages. */
	public connectAppSocket(handlers: AppSocketHandlers): WebSocket {
		const socket = new WebSocket(this.wsUrl('/api/v1/monitor/ws'));
		this.attachWebSocketLogging(socket);
		socket.onmessage = (event) => this.handleAppSocketMessage(event, handlers);
		if (handlers.onClose) socket.onclose = handlers.onClose;
		return socket;
	}

	/** Start monitoring the given source. */
	public startMonitor(sourceName: string): Promise<void> {
		return this.postJsonVoid('/api/v1/monitor/start', { sourceName });
	}

	/** Stop monitoring. */
	public stopMonitor(): Promise<void> {
		return this.postVoid('/api/v1/monitor/stop');
	}

	private getJson<T>(path: string, init?: RequestInit): Promise<T> {
		return this.json<T>(path, init);
	}

	private post<T>(path: string, init?: RequestInit, errors?: RequestErrorMessages): Promise<T> {
		return this.json<T>(path, { method: 'POST', ...init }, errors);
	}

	private postVoid(path: string, errors?: RequestErrorMessages): Promise<void> {
		return this.void(path, { method: 'POST' }, errors);
	}

	private postJson<T>(path: string, body: unknown, init?: RequestInit, errors?: RequestErrorMessages): Promise<T> {
		return this.json<T>(path, this.withJsonBody('POST', body, init), errors);
	}

	private postJsonVoid(path: string, body: unknown, init?: RequestInit, errors?: RequestErrorMessages): Promise<void> {
		return this.void(path, this.withJsonBody('POST', body, init), errors);
	}

	private putJson<T>(path: string, body: unknown): Promise<T> {
		return this.json<T>(path, this.withJsonBody('PUT', body));
	}

	private patchJson<T>(path: string, body: unknown): Promise<T> {
		return this.json<T>(path, this.withJsonBody('PATCH', body));
	}

	private delete(path: string): Promise<void> {
		return this.void(path, { method: 'DELETE' });
	}

	private async json<T>(path: string, init?: RequestInit, errors?: RequestErrorMessages): Promise<T> {
		const res = await this.request(path, init, errors);
		return res.json() as Promise<T>;
	}

	private async void(path: string, init?: RequestInit, errors?: RequestErrorMessages): Promise<void> {
		await this.request(path, init, errors);
	}

	private async request(path: string, init?: RequestInit, errors?: RequestErrorMessages): Promise<Response> {
		const res = await fetch(this.apiUrl(path), init);
		if (errors?.[res.status]) throw new Error(errors[res.status]);
		if (!res.ok) throw new Error(`Request error: ${res.status} ${await res.text()}`);
		return res;
	}

	private withJsonBody(method: string, body: unknown, init: RequestInit = {}): RequestInit {
		const headers = new Headers(init.headers);
		headers.set('content-type', 'application/json');
		return { ...init, method, headers, body: JSON.stringify(body) };
	}

	private query(params: Record<string, string>): string {
		return new URLSearchParams(params).toString();
	}

	private bool(value: boolean | undefined): string {
		return value ? 'true' : 'false';
	}

	private browserWsLogEnabled(): boolean {
		return document.querySelector('meta[name="ge-browser-ws-log"]')?.getAttribute('content') === '1';
	}

	private attachWebSocketLogging(socket: WebSocket): void {
		if (!this.browserWsLogEnabled()) return;
		const url = socket.url;
		console.debug('[GE websocket] connecting', { url });
		socket.addEventListener('open', () => console.debug('[GE websocket] open', { url }));
		socket.addEventListener('close', (event) => {
			console.debug('[GE websocket] close', { url, code: event.code, reason: event.reason, wasClean: event.wasClean });
		});
		socket.addEventListener('error', (event) => console.debug('[GE websocket] error', { url, event }));

		const send = socket.send.bind(socket);
		socket.send = (data: Parameters<WebSocket['send']>[0]) => {
			console.debug('[GE websocket] send', data);
			send(data);
		};
	}

	private reloadIfStale(backendBuildId: string): void {
		const self = this.selfBuildId();
		if (self !== null && self !== backendBuildId) {
			console.warn(`frontend build ${self} differs from backend build ${backendBuildId}; reloading`);
			window.location.reload();
		}
	}

	private handleAppSocketMessage(event: MessageEvent, handlers: AppSocketHandlers): void {
		if (this.browserWsLogEnabled()) console.debug('[GE websocket] receive raw', event.data);
		const msg = JSON.parse(event.data) as AppSocketEvent;
		if (this.browserWsLogEnabled()) console.debug('[GE websocket] receive parsed', msg);
		switch (msg.type) {
			case 'version':
				if (typeof msg.buildId === 'string') this.reloadIfStale(msg.buildId);
				else console.warn('Ignoring malformed monitor version event', msg);
				break;
			case 'snapshot':
				if (msg.state && typeof msg.state === 'object') handlers.onSnapshot?.(msg.state);
				else console.warn('Ignoring malformed snapshot event', msg);
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
				if (typeof msg.version === 'string') handlers.onUpdateApplied?.(msg.version, msg.releaseUrl);
				else console.warn('Ignoring malformed updateApplied event', msg);
				break;
			case 'updateStagingFailed':
				handlers.onUpdateStagingFailed?.(msg.error);
				break;
			case 'youtubeUploadChanged':
				handlers.onYoutubeUploadChanged?.(msg.upload);
				break;
		}
	}
}

export const backend = new Backend();
export interface ObsSource {
	name: string;
	id: string;
}

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

export interface EditableRunMetadata {
	romLanguage: string;
	status: string;
	difficulty: string;
	time: string;
	level: string;
}

export type YouTubeUploadState = 'queued' | 'uploading' | 'processing' | 'uploaded' | 'failed';

export interface YouTubeUploadStatus {
	id: string;
	path: string;
	fileName: string;
	state: YouTubeUploadState;
	progressBytes: number;
	totalBytes: number | null;
	progressRatio: number | null;
	videoId: string | null;
	videoUrl: string | null;
	error: string | null;
	title: string;
	startedAt: string;
	finishedAt: string | null;
}

export interface YouTubeUploadHistoryEntry {
	path: string;
	videoId: string;
	videoUrl: string;
	uploadedAt: string;
	title: string;
}

export interface YouTubeAccount {
	email: string | null;
	name: string | null;
	picture: string | null;
}

export interface YouTubeStatus {
	enabled: boolean;
	oauthConfigured: boolean;
	connected: boolean;
	account: YouTubeAccount | null;
	uploads: YouTubeUploadStatus[];
	history: YouTubeUploadHistoryEntry[];
}

export interface YouTubeUploadOptions {
	datetimeLocal?: string;
}

/** Replay-buffer status mirrors Rust `ReplayBufferStatus`. */
export interface ReplayBufferStatus {
	enabled: boolean;
	available: boolean;
	active: boolean;
	maxSeconds: number | null;
	outputDirectory: string | null;
	defaultCompletedOutputPath: string | null;
	defaultFailedOutputPath: string | null;
}

export interface SettingsStatus {
	settings: Settings;
	defaults: Settings;
	configPath: string;
	pluginVersion: string;
	fileError?: string | null;
}

export interface PluginUpdate {
	currentVersion: string;
	latestVersion: string;
	releaseUrl: string;
}

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

/** The level match the backend pushes over the monitor WebSocket. */
export interface LevelMatch {
	screen: string;
	mission: number;
	part: number;
	difficulty: number;
	detected_lang?: 'en' | 'jp';
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

/** Details of a clip the backend saved out of the replay buffer. */
export interface RecordingSaved {
	saveId: number;
	path: string;
	replayPath: string;
	durationSecs: number;
	failed: boolean;
	stats?: LevelMatch;
}

/** A scheduled save that was dropped before any clip was written. */
export interface RecordingSaveDiscarded {
	saveId: number;
}

/** Details of a clip save that has been scheduled after a run ending was seen. */
export interface RecordingSavePending {
	saveId: number;
	saveInSecs: number;
	estimatedDurationSecs: number;
	failed: boolean;
	status: string;
	level: string;
	levelNumber?: number;
	difficulty?: string;
	timeSecs?: number;
	targetTimeSecs?: number;
	bestTimeSecs?: number;
	stats?: LevelMatch;
}

/** Recording configuration stored by the Rust backend. */
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

/** A transition in the recorder's per-run state. Mirrors Rust `RecordingStatus`. */
export type RecordingStatus =
	| 'started'
	| 'cancelled'
	| 'failed'
	| 'aborted'
	| 'kia'
	| 'complete'
	| 'statsSkipped'
	| 'savePending';

/** Why a failed run reached an ending screen without a clip being saved. */
export type FailedRunNotSavedReason = 'savingDisabled' | 'tooShort';

/** Why the backend stopped monitoring. Mirrors the Rust `MonitorStoppedReason`. */
export type MonitorStoppedReason = 'userStopped' | 'replayBufferStopped';

export interface MonitorFps {
	processedFps: number;
	sourceFps: number;
}

/** A message pushed over the app WebSocket. Mirrors the Rust `MonitorEvent`. */
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
	| { type: 'updateStagingFailed'; error: string }
	| { type: 'youtubeUploadChanged'; upload: YouTubeUploadStatus };

/** Handlers for the messages the app WebSocket can push. All are optional. */
export interface AppSocketHandlers {
	onSnapshot?: (snapshot: AppSnapshot) => void;
	onLanguageDetected?: (lang: 'en' | 'jp') => void;
	onMonitorFps?: (fps: MonitorFps) => void;
	onRecordingSavePending?: (pending: RecordingSavePending) => void;
	onRecordingSaved?: (saved: RecordingSaved) => void;
	onRecordingSaveDiscarded?: (discarded: RecordingSaveDiscarded) => void;
	onFailedRunNotSaved?: (reason: FailedRunNotSavedReason) => void;
	onMonitorStopped?: (reason: MonitorStoppedReason) => void;
	onSettingsReloaded?: (settings: Settings, configPath: string) => void;
	onSettingsInvalid?: (error: string, configPath: string) => void;
	onUpdateApplied?: (version: string, releaseUrl?: string) => void;
	onUpdateStagingFailed?: (error: string) => void;
	onYoutubeUploadChanged?: (upload: YouTubeUploadStatus) => void;
	onClose?: () => void;
}

export type MonitorStatus =
	| { enabled: false; recordingState?: null }
	| { enabled: true; sourceName: string; recordingState: RecordingStatus | null };
