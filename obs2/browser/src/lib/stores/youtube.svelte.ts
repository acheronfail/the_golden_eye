import { addNotificationFlag } from '$lib/stores/notifications.svelte';
import {
	backend,
	type YouTubeStatus,
	type YouTubeAccount,
	type YouTubeUploadHistoryEntry,
	type YouTubeUploadStatus
} from '$lib/api';

const errorMessage = (err: unknown): string => (err instanceof Error ? err.message : String(err));
const terminalUploadStates = new Set(['uploaded', 'failed', 'cancelled']);

const currentPlatform = (): string => (typeof navigator === 'undefined' ? '' : navigator.platform.toLowerCase());

export const youtubePathKeyForPlatform = (path: string, platform: string): string => {
	const normalized = path.replaceAll('\\', '/');
	const normalizedPlatform = platform.toLowerCase();
	return normalizedPlatform.includes('mac') || normalizedPlatform.includes('win')
		? normalized.toLowerCase()
		: normalized;
};

export const youtubePathsMatchForPlatform = (a: string, b: string, platform: string): boolean =>
	youtubePathKeyForPlatform(a, platform) === youtubePathKeyForPlatform(b, platform);

const pathsMatch = (a: string, b: string): boolean => youtubePathsMatchForPlatform(a, b, currentPlatform());

export const youtube = new (class {
	loaded = $state(false);
	loading = $state(false);
	connecting = $state(false);
	cancelling = $state(false);
	disconnecting = $state(false);
	error = $state<string | null>(null);
	enabled = $state(false);
	oauthConfigured = $state(false);
	connected = $state(false);
	account = $state<YouTubeAccount | null>(null);
	uploads = $state<YouTubeUploadStatus[]>([]);
	history = $state<YouTubeUploadHistoryEntry[]>([]);
	cancellingUploadIds = $state<string[]>([]);
	uploadCancelErrors = $state<Record<string, string>>({});
	private notifiedFailedIds = new Set<string>();

	async load(): Promise<void> {
		this.loading = true;
		this.error = null;
		try {
			this.applyStatus(await backend.getYouTubeStatus());
		} catch (err) {
			this.error = errorMessage(err);
			throw err;
		} finally {
			this.loading = false;
		}
	}

	async connect(): Promise<void> {
		this.connecting = true;
		this.error = null;
		try {
			this.applyStatus(await backend.connectYouTube());
		} catch (err) {
			// A cancelled flow is expected when the user clicks Cancel; not an error.
			if (!this.cancelling) {
				this.error = errorMessage(err);
				throw err;
			}
		} finally {
			this.connecting = false;
			this.cancelling = false;
		}
	}

	async cancel(): Promise<void> {
		this.cancelling = true;
		try {
			this.applyStatus(await backend.cancelYouTubeConnect());
		} catch (err) {
			this.error = errorMessage(err);
			throw err;
		}
	}

	async disconnect(): Promise<void> {
		this.disconnecting = true;
		this.error = null;
		try {
			this.applyStatus(await backend.disconnectYouTube());
		} catch (err) {
			this.error = errorMessage(err);
			throw err;
		} finally {
			this.disconnecting = false;
		}
	}

	async upload(path: string, options?: { datetimeLocal?: string }): Promise<YouTubeUploadStatus> {
		this.error = null;
		try {
			const status = await backend.uploadRunToYouTube(path, options);
			this.applyUpload(status);
			return status;
		} catch (err) {
			this.error = errorMessage(err);
			throw err;
		}
	}

	async cancelUpload(id: string): Promise<void> {
		if (this.cancellingUploadIds.includes(id)) return;
		this.cancellingUploadIds = [...this.cancellingUploadIds, id];
		delete this.uploadCancelErrors[id];
		try {
			this.handleUploadChanged(await backend.cancelYouTubeUpload(id));
		} catch (err) {
			this.uploadCancelErrors[id] = errorMessage(err);
		} finally {
			this.cancellingUploadIds = this.cancellingUploadIds.filter((pending) => pending !== id);
		}
	}

	async forget(path: string): Promise<void> {
		this.error = null;
		try {
			this.applyStatus(await backend.forgetYouTubeUpload(path));
		} catch (err) {
			this.error = errorMessage(err);
			throw err;
		}
	}

	applyStatus(status: YouTubeStatus): void {
		this.enabled = status.enabled;
		this.oauthConfigured = status.oauthConfigured;
		this.connected = status.connected;
		this.account = status.account;
		this.uploads = status.uploads;
		this.history = status.history;
		this.loaded = true;
	}

	handleUploadChanged(upload: YouTubeUploadStatus): void {
		const previous = this.uploads.find((item) => item.id === upload.id);
		if (
			upload.state === 'uploaded' &&
			previous?.state !== 'uploaded' &&
			(previous?.state === 'cancelling' || this.cancellingUploadIds.includes(upload.id))
		) {
			addNotificationFlag({
				key: `youtube-completed-${upload.id}`,
				title: 'Upload completed before cancellation',
				detail: upload.title || upload.fileName,
				tone: 'info',
				timeoutMs: 8000,
				href: `/runs?runId=${encodeURIComponent(upload.runId)}`
			});
		}
		this.applyUpload(upload);
		if (upload.state === 'failed' && !this.notifiedFailedIds.has(upload.id)) {
			this.notifiedFailedIds.add(upload.id);
			addNotificationFlag({
				key: `youtube-upload-${upload.id}`,
				title: 'YouTube upload failed',
				detail: upload.title || upload.fileName,
				meta: 'Click here to view the run.',
				tone: 'error' as const,
				timeoutMs: 8000,
				href: `/runs?runId=${encodeURIComponent(upload.runId)}`
			});
		}
	}

	handlePersonalBestUploadStarted(upload: YouTubeUploadStatus): void {
		addNotificationFlag({
			key: `youtube-pb-${upload.id}`,
			title: 'Personal best detected — upload started',
			detail: upload.title || upload.fileName,
			meta: 'Your run was added to the YouTube upload queue. Click to view it.',
			tone: 'success',
			timeoutMs: 8000,
			href: `/runs?runId=${encodeURIComponent(upload.runId)}`
		});
	}

	applyUpload(status: YouTubeUploadStatus): void {
		const previous = this.uploads.find((upload) => upload.id === status.id);
		if (
			previous &&
			(terminalUploadStates.has(previous.state) ||
				(previous.state === 'cancelling' && !terminalUploadStates.has(status.state)))
		)
			return;
		const next = this.uploads.filter((upload) => upload.id !== status.id);
		next.push(status);
		next.sort((a, b) => a.startedAt.localeCompare(b.startedAt));
		this.uploads = next;
	}

	uploadForRun(runId: string): YouTubeUploadStatus | null {
		const matches = this.uploads.filter((upload) => upload.runId === runId);
		const active = matches.findLast((upload) => !terminalUploadStates.has(upload.state));
		return active ?? matches.at(-1) ?? null;
	}

	historyForPath(path: string): YouTubeUploadHistoryEntry | null {
		const matches = this.history.filter((entry) => pathsMatch(entry.path, path));
		return matches.at(-1) ?? null;
	}
})();
