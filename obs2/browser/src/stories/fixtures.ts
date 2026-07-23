import type {
	EditableRunMetadata,
	RunClip,
	RunDirectoryScan,
	YouTubeStatus,
	YouTubeUploadHistoryEntry,
	YouTubeUploadState,
	YouTubeUploadStatus
} from '$lib/api';
import type { NotificationFlag } from '$lib/stores/notifications.svelte';

export const completedRun: RunClip = {
	path: '/runs/completed/2026-07-21-facility-00-agent-00-58.mp4',
	fileName: '2026-07-21 - Facility - 00 Agent - 00-58.mp4',
	directory: '/runs/completed',
	sizeBytes: 148_700_000,
	modified: '2026-07-21T12:45:04Z',
	durationSecs: 75.4,
	metadata: {
		timestamp: '2026-07-21T12:43:09Z',
		time: '00:58',
		timeSeconds: 58,
		level: 'Facility',
		levelNumber: 2,
		difficulty: '00 Agent',
		status: 'complete',
		romLanguage: 'en',
		sourceName: 'Nintendo 64',
		comment: '',
		pluginVersion: '2.4.0'
	}
};

export const failedRun: RunClip = {
	...completedRun,
	path: '/runs/failed/2026-07-20-control-agent-kia.mp4',
	fileName: '2026-07-20 - Control - Agent - KIA.mp4',
	directory: '/runs/failed',
	sizeBytes: 92_300_000,
	durationSecs: 44.8,
	metadata: {
		...completedRun.metadata,
		timestamp: '2026-07-20T08:17:31Z',
		time: '00:37',
		timeSeconds: 37,
		level: 'Control',
		levelNumber: 17,
		difficulty: 'Agent',
		status: 'kia',
		romLanguage: 'jp'
	}
};

export const abortedRun: RunClip = {
	...completedRun,
	path: '/runs/failed/2026-07-19-dam-secret-agent-aborted.mp4',
	fileName: '2026-07-19 - Dam - Secret Agent - aborted.mp4',
	directory: '/runs/failed',
	sizeBytes: 31_400_000,
	durationSecs: 21.2,
	metadata: {
		...completedRun.metadata,
		timestamp: '2026-07-19T22:02:14Z',
		time: '00:14',
		timeSeconds: 14,
		level: 'Dam',
		levelNumber: 1,
		difficulty: 'Secret Agent',
		status: 'abort'
	}
};

export const untaggedRun: RunClip = {
	...completedRun,
	path: '/runs/completed/replay-buffer-save.mp4',
	fileName: 'Replay Buffer - 2026-07-18 1032.mp4',
	metadata: {
		...completedRun.metadata,
		timestamp: '2026-07-18T10:32:00Z',
		time: '',
		timeSeconds: undefined,
		level: '',
		levelNumber: undefined,
		difficulty: '',
		status: '',
		romLanguage: ''
	}
};

export const runClips = [completedRun, failedRun, abortedRun, untaggedRun];

export const draftForRun = (clip: RunClip): EditableRunMetadata => ({
	romLanguage: clip.metadata.romLanguage,
	status: clip.metadata.status,
	difficulty: clip.metadata.difficulty ?? '',
	time: clip.metadata.time ?? '',
	level: clip.metadata.level
});

export const completedDirectory: RunDirectoryScan = {
	kind: 'completed',
	path: '/runs/completed',
	exists: true
};

export const youtubeStatus = (overrides: Partial<YouTubeStatus> = {}): YouTubeStatus => ({
	enabled: true,
	oauthConfigured: true,
	connected: false,
	account: null,
	uploads: [],
	history: [],
	...overrides
});

export const connectedYouTube = youtubeStatus({
	connected: true,
	account: {
		name: 'Natalya Simonova',
		email: 'natalya@example.com',
		picture: null
	}
});

export const uploadForRun = (
	state: YouTubeUploadState,
	overrides: Partial<YouTubeUploadStatus> = {}
): YouTubeUploadStatus => ({
	id: `upload-${state}`,
	path: completedRun.path,
	fileName: completedRun.fileName,
	state,
	progressBytes: state === 'uploading' ? 72_000_000 : 0,
	totalBytes: 148_700_000,
	progressRatio: state === 'uploading' ? 0.484 : null,
	videoId: state === 'uploaded' ? 'dQw4w9WgXcQ' : null,
	videoUrl: state === 'uploaded' ? 'https://youtu.be/dQw4w9WgXcQ' : null,
	error: state === 'failed' ? 'The upload session expired before all bytes were transferred.' : null,
	title: 'Facility - 00 Agent - 00:58',
	startedAt: '2026-07-21T12:47:00Z',
	finishedAt: state === 'uploaded' || state === 'failed' ? '2026-07-21T12:49:12Z' : null,
	...overrides
});

export const uploadedHistory: YouTubeUploadHistoryEntry = {
	path: completedRun.path,
	videoId: 'dQw4w9WgXcQ',
	videoUrl: 'https://youtu.be/dQw4w9WgXcQ',
	uploadedAt: '2026-07-21T12:49:12Z',
	title: 'Facility - 00 Agent - 00:58'
};

export const notificationFixtures: NotificationFlag[] = [
	{
		id: 101,
		title: 'Monitor started',
		detail: 'Watching Nintendo 64 for a new run.',
		tone: 'info'
	},
	{
		id: 102,
		title: 'Run saved',
		detail: completedRun.fileName,
		meta: 'Click to open the run.',
		pills: [{ label: 'Facility' }, { label: '00:58' }, { label: '00 Agent' }],
		tone: 'success',
		href: '/runs'
	},
	{
		id: 103,
		title: 'Replay buffer time is short',
		detail: 'OBS is configured for 300 seconds; 1200 seconds is recommended.',
		tone: 'warning'
	},
	{
		id: 104,
		title: 'YouTube upload failed',
		detail: 'The upload session expired before all bytes were transferred.',
		meta: 'The clip is still available locally.',
		tone: 'error'
	}
];

const notice = (id: number, options: Omit<NotificationFlag, 'id'>): NotificationFlag => ({ id, ...options });
const actionable = () => {};

export const notificationScenarios = {
	languageDetected: [
		notice(201, {
			title: 'ROM language detected',
			detail: 'Japanese templates are active for this source.',
			meta: 'Monitoring will switch automatically if needed.',
			tone: 'info'
		})
	],
	monitoringDisabled: [
		notice(206, {
			title: 'Monitoring disabled',
			detail: "OBS's replay buffer was unexpectedly stopped.",
			meta: 'Monitoring was disabled because clips can no longer be saved.',
			tone: 'error'
		})
	],
	youtubeStarted: [
		notice(207, {
			title: 'YouTube upload started',
			detail: 'Facility - 00 Agent - 00:58',
			tone: 'info'
		})
	],
	youtubeFailed: [
		notice(208, {
			title: 'YouTube upload failed',
			detail: 'Facility - 00 Agent - 00:58',
			meta: 'An error occurred when trying to upload the video.',
			tone: 'error',
			timeoutMs: 600_000
		})
	],
	youtubeCompleted: [
		notice(209, {
			title: 'YouTube upload completed',
			detail: 'Facility - 00 Agent - 00:58',
			meta: 'Click here to open YouTube.',
			tone: 'success',
			timeoutMs: 600_000,
			action: actionable
		})
	],
	configInvalid: [
		notice(210, {
			title: 'Config file invalid',
			detail: 'settings.json contains invalid JSON at line 12.',
			meta: 'Click here to open options.',
			tone: 'error',
			href: '/options'
		})
	],
	configReloaded: [
		notice(211, {
			title: 'Config reloaded',
			detail: '/Users/bond/Library/Application Support/The Golden Eye/settings.json',
			tone: 'success',
			timeoutMs: 600_000
		})
	],
	updateAvailable: [
		notice(212, {
			title: 'Plugin update available',
			detail: '2.4.0 -> 2.5.0',
			meta: 'Click here to download and install.',
			tone: 'info',
			action: actionable
		})
	],
	updateDownloading: [
		notice(213, {
			title: 'Downloading update',
			detail: 'Downloading and verifying 2.5.0...',
			tone: 'info'
		})
	],
	updateReady: [
		notice(214, {
			title: 'Update ready',
			detail: 'The verified update is ready to apply.',
			meta: 'Click here to apply the update.',
			tone: 'success',
			action: actionable
		})
	],
	updateApplying: [
		notice(215, {
			title: 'Applying update',
			detail: 'The plugin will briefly reconnect while the update is installed.',
			tone: 'success'
		})
	],
	updateApplied: [
		notice(216, {
			title: 'Plugin updated',
			detail: 'Now running v2.5.0',
			meta: 'Click here to view the changelog.',
			tone: 'success',
			action: actionable
		})
	],
	updateFailed: [
		notice(217, {
			title: 'Plugin update failed',
			detail: 'The downloaded package did not match its expected checksum.',
			tone: 'error'
		})
	],
	updateCheckFailed: [
		notice(218, {
			title: 'Update check failed',
			detail: 'GitHub could not be reached. Check your network connection and try again.',
			tone: 'error'
		})
	],
	updateDownloadFailed: [
		notice(219, {
			title: 'Update download failed',
			detail: 'The release archive could not be downloaded.',
			tone: 'error'
		})
	],
	updateApplyFailed: [
		notice(220, {
			title: 'Could not apply update',
			detail: 'No staged update is available to apply.',
			tone: 'error'
		})
	],
	updateCheckResult: [
		notice(221, {
			title: "You're up to date",
			tone: 'success',
			timeoutMs: 600_000
		})
	]
} satisfies Record<string, NotificationFlag[]>;
