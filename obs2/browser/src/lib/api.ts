import type { Settings } from '$lib/stores/settings.svelte';
import type {
	AppEvent,
	AppSnapshot,
	AnnotationRect,
	AnnotationSet,
	ActivePictureRegion,
	BlackFrameSignal,
	BucketCounts,
	ClipMetadata,
	DifficultyNumber,
	FolderPickResult,
	FolderValidation,
	LevelMatch,
	LevelTimerPhase,
	LevelTimerStartReason,
	ManualRunInput,
	MatchSourceResponse,
	MonitorFps,
	MonitorSnapshot,
	MonitorStoppedReason,
	MonitorWallClockState,
	MonitoringSessionDetail,
	MonitoringSessionSummary,
	ObsSource,
	PluginUpdate,
	RecordingOptions,
	RecordingSavePending,
	RecordingSaved,
	RecordingStatus,
	ReplayBufferStatus,
	ReplaySaveStage,
	ReplaySaveStatus,
	RomVersion,
	RunCatalogSync,
	RunClip,
	RunDirectoryScan,
	RunRetentionState,
	RunYouTubeVideo,
	RunsResponse,
	RunSort,
	RunStatus,
	SettingsStatus,
	StatisticsBucket,
	StatisticsResponse,
	StatusCounts,
	TheEliteImportResponse,
	UpdatePhase,
	UpdateStatus,
	YouTubeAccount,
	YouTubeAssociationSource,
	YouTubeStatus,
	YouTubeUploadHistoryEntry,
	YouTubeUploadState,
	YouTubeUploadStatus
} from '$lib/generated/api';

// In dev the SPA is Vite-served on its own port, so
// point API calls at that absolute origin. Production serves the SPA itself, so
// relative URLs stay origin-agnostic.
const API_ORIGIN = import.meta.env.DEV ? `http://localhost:${import.meta.env.VITE_GE_SERVER_PORT}` : '';

type RequestErrorMessages = Partial<Record<number, string>>;

interface RunQueryFilters {
	search: string;
	level: string;
	difficulty: string;
	status: string;
	language: string;
	minTime: string;
	maxTime: string;
}

const runTimeSeconds = (value?: string): number | null => {
	if (!value?.trim()) return null;
	const parts = value.trim().split(':').map(Number);
	if (parts.length === 1 && Number.isFinite(parts[0])) return Math.max(0, parts[0]);
	if (parts.length !== 2 || parts.some((part) => !Number.isFinite(part))) return null;
	return parts[1] >= 0 && parts[1] < 60 ? Math.max(0, parts[0] * 60 + parts[1]) : null;
};

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
		if (import.meta.env.DEV) return `ws://localhost:${import.meta.env.VITE_GE_SERVER_PORT}${path}`;
		const proto = window.location.protocol === 'https:' ? 'wss:' : 'ws:';
		return `${proto}//${window.location.host}${path}`;
	}

	/** URL for a single-frame screenshot of the given OBS source, usable as `<img src>`. */
	public screenshotUrl(source: string): string {
		return this.apiUrl(`/api/v1/screenshot?source=${encodeURIComponent(source)}`);
	}

	public getRuns(
		options: {
			refresh?: boolean;
			sort?: RunSort;
			cursor?: string;
			runId?: string;
			limit?: number;
			filters?: RunQueryFilters;
			signal?: AbortSignal;
		} = {}
	): Promise<RunsResponse> {
		const query = new URLSearchParams();
		if (options.refresh) query.set('refresh', 'true');
		query.set('sort', options.sort ?? 'newest');
		if (options.cursor) query.set('cursor', options.cursor);
		if (options.runId) query.set('runId', options.runId);
		if (options.limit) query.set('limit', String(options.limit));
		const filters = options.filters;
		if (filters?.search.trim()) query.set('search', filters.search.trim());
		if (filters?.level) query.set('level', filters.level);
		if (filters?.difficulty) query.set('difficulty', filters.difficulty);
		if (filters?.status) query.set('status', filters.status);
		if (filters?.language) query.set('language', filters.language);
		const minimum = runTimeSeconds(filters?.minTime);
		const maximum = runTimeSeconds(filters?.maxTime);
		if (minimum !== null) query.set('minTimeSeconds', String(minimum));
		if (maximum !== null) query.set('maxTimeSeconds', String(maximum));
		return this.getJson(`/api/v1/runs?${query}`, { signal: options.signal });
	}

	public getRecentRuns(limit?: number): Promise<RunClip[]> {
		const query = limit == null ? '' : `?limit=${encodeURIComponent(limit)}`;
		return this.getJson(`/api/v1/runs/recent${query}`);
	}

	public keepRun(runId: string): Promise<RunClip> {
		return this.postJson('/api/v1/runs/keep', { runId });
	}

	public createManualRun(input: ManualRunInput): Promise<RunClip> {
		return this.postJson('/api/v1/runs/manual', input);
	}

	public importTheElite(username: string): Promise<TheEliteImportResponse> {
		return this.postJson('/api/v1/runs/import/the-elite', { username });
	}

	public deleteCatalogRun(runId: string, keepHistory: boolean): Promise<RunClip | null> {
		return this.postJson('/api/v1/runs/delete', { runId, keepHistory });
	}

	public runVideoUrl(path: string): string {
		return this.apiUrl(`/api/v1/runs/video?path=${encodeURIComponent(path)}`);
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

	public cancelYouTubeConnect(): Promise<YouTubeStatus> {
		return this.post('/api/v1/youtube/cancel');
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

	public updateRunMetadata(runId: string, metadata: EditableRunMetadata): Promise<RunClip> {
		return this.patchJson('/api/v1/runs', {
			runId,
			metadata: { ...metadata, romVersion: metadata.romVersion || null }
		});
	}

	/** Fetch whether OBS's replay buffer is enabled/available (and running). */
	public getReplayBufferStatus(): Promise<ReplayBufferStatus> {
		return this.getJson('/api/v1/replay-buffer/status');
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
	public getUpdateStatus(): Promise<UpdateStatus> {
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

	/** Open the app event stream. */
	public connectAppSocket(onEvent: (event: AppEvent) => void, onClose: () => void): WebSocket {
		const socket = new WebSocket(this.wsUrl('/api/v1/events/ws'));
		this.attachWebSocketLogging(socket);
		socket.onmessage = (event) => this.handleAppSocketMessage(event, onEvent);
		socket.onclose = onClose;
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

	public getStatistics(
		filters: StatisticsFilters,
		options: { signal?: AbortSignal } = {}
	): Promise<StatisticsResponse> {
		const query = new URLSearchParams();
		if (filters.from) query.set('from', filters.from);
		if (filters.to) query.set('to', filters.to);
		query.set('bucket', filters.bucket);
		if (filters.levelNumber != null) query.set('levelNumber', String(filters.levelNumber));
		if (filters.difficultyNumber != null) query.set('difficultyNumber', String(filters.difficultyNumber));
		return this.getJson(`/api/v1/statistics?${query}`, { signal: options.signal });
	}

	public getStatisticsSessions(
		range: Pick<StatisticsFilters, 'from' | 'to'>,
		options: { signal?: AbortSignal } = {}
	): Promise<MonitoringSessionSummary[]> {
		const query = new URLSearchParams();
		if (range.from) query.set('from', range.from);
		if (range.to) query.set('to', range.to);
		return this.getJson(`/api/v1/statistics/sessions?${query}`, { signal: options.signal });
	}

	public getStatisticsSession(
		sessionId: string,
		options: { signal?: AbortSignal } = {}
	): Promise<MonitoringSessionDetail> {
		return this.getJson(`/api/v1/statistics/sessions/${encodeURIComponent(sessionId)}`, {
			signal: options.signal
		});
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

	private handleAppSocketMessage(event: MessageEvent, onEvent: (event: AppEvent) => void): void {
		if (this.browserWsLogEnabled()) console.debug('[GE websocket] receive raw', event.data);
		try {
			const message = JSON.parse(event.data) as unknown;
			if (!message || typeof message !== 'object' || !('type' in message) || typeof message.type !== 'string') {
				console.warn('Ignoring malformed app event', message);
				return;
			}
			if (this.browserWsLogEnabled()) console.debug('[GE websocket] receive parsed', message);
			onEvent(message as AppEvent);
		} catch (error) {
			console.warn('Ignoring invalid app event JSON', error);
		}
	}
}

export const backend = new Backend();

export type {
	AppEvent,
	AppSnapshot,
	AnnotationRect,
	AnnotationSet,
	ActivePictureRegion,
	BlackFrameSignal,
	BucketCounts,
	ClipMetadata,
	DifficultyNumber,
	FolderPickResult,
	FolderValidation,
	LevelMatch,
	LevelTimerPhase,
	LevelTimerStartReason,
	ManualRunInput,
	MatchSourceResponse,
	MonitorFps,
	MonitorSnapshot,
	MonitorStoppedReason,
	MonitorWallClockState,
	MonitoringSessionDetail,
	MonitoringSessionSummary,
	ObsSource,
	PluginUpdate,
	RecordingOptions,
	RecordingSavePending,
	RecordingSaved,
	RecordingStatus,
	ReplayBufferStatus,
	ReplaySaveStage,
	ReplaySaveStatus,
	RomVersion,
	RunCatalogSync,
	RunClip,
	RunDirectoryScan,
	RunRetentionState,
	RunYouTubeVideo,
	RunsResponse,
	RunSort,
	RunStatus,
	SettingsStatus,
	StatisticsBucket,
	StatisticsResponse,
	StatusCounts,
	TheEliteImportResponse,
	UpdatePhase,
	UpdateStatus,
	YouTubeAccount,
	YouTubeAssociationSource,
	YouTubeStatus,
	YouTubeUploadHistoryEntry,
	YouTubeUploadState,
	YouTubeUploadStatus
};

export const DIFFICULTY_LABELS: Record<DifficultyNumber, string> = {
	0: 'Agent',
	1: 'Secret Agent',
	2: '00 Agent',
	3: '007'
};

export interface StatisticsFilters {
	from?: string;
	to: string;
	bucket: StatisticsBucket;
	levelNumber?: number;
	difficultyNumber?: DifficultyNumber;
}

export interface EditableRunMetadata {
	gameLanguage: string;
	romVersion: RomVersion | '';
	status: string;
	difficulty: string;
	time: string;
	level: string;
}

export interface YouTubeUploadOptions {
	datetimeLocal?: string;
}

export type MonitorStatus =
	| { enabled: false; recordingState?: null }
	| { enabled: true; sourceName: string; recordingState: RecordingStatus | null };
