import { browser } from '$app/environment';
import { backend, type AppEvent, type AppSnapshot, type PluginUpdate } from './api';
import {
	applyFailedRunNotSaved,
	applyLanguageDetected,
	applyMonitorFps,
	applyMonitorSnapshot,
	applyMonitorStopped,
	applyRecordingSaved,
	applyRecordingSaveDiscarded,
	applyRecordingSavePending
} from './monitor.svelte';
import {
	addNotificationFlag,
	dismissNotificationFlag,
	dismissNotificationFlagsByKey,
	removeNotificationFlag,
	replaceNotificationFlag
} from './notifications.svelte';
import { refreshReplayBuffer, setReplayBufferStatus } from './replayBuffer.svelte';
import { settings } from './settings.svelte';
import { setObsSources } from './sources.svelte';
import { youtube } from './youtube.svelte';

let socket: WebSocket | null = null;
let reconnectTimer: ReturnType<typeof setTimeout> | null = null;
let stopped = true;
let settingsErrorNotificationId: number | null = null;
let updateNotificationId: number | null = null;
let dismissedUpdateVersion: string | null = null;
const youtubeStartedNotificationIds = new Map<string, number>();
const youtubeNotifiedCompletedIds = new Set<string>();
const youtubeNotifiedFailedIds = new Set<string>();
const UPDATE_APPLIED_STORAGE_KEY = 'ge-update-applied-version';

const showUpdateAppliedNotification = (version: string, releaseUrl?: string): void => {
	addNotificationFlag({
		key: 'plugin-update-applied',
		title: 'Plugin updated',
		detail: `Now running v${version}`,
		tone: 'success',
		meta: releaseUrl ? 'Click to view the changelog.' : undefined,
		action: releaseUrl
			? async () => {
					try {
						await backend.openUpdateRelease(releaseUrl);
					} catch (err) {
						console.warn('Failed to open plugin update release', err);
					}
				}
			: undefined
	});
};

/** In production the page is about to reload after `updateApplied`, so persist
 * the toast across the reload and show it once on the fresh page. Dev never
 * reloads (no `ge-build-id` meta tag), so there it must show immediately. */
const handleUpdateApplied = (version: string, releaseUrl?: string): void => {
	if (backend.selfBuildId() === null) {
		showUpdateAppliedNotification(version, releaseUrl);
		return;
	}
	try {
		sessionStorage.setItem(UPDATE_APPLIED_STORAGE_KEY, JSON.stringify({ version, releaseUrl }));
	} catch (err) {
		console.warn('Failed to persist pending update-applied notice', err);
	}
};

const consumePendingUpdateAppliedNotification = (): void => {
	let stored: string | null = null;
	try {
		stored = sessionStorage.getItem(UPDATE_APPLIED_STORAGE_KEY);
		if (stored !== null) sessionStorage.removeItem(UPDATE_APPLIED_STORAGE_KEY);
	} catch (err) {
		console.warn('Failed to read pending update-applied notice', err);
	}
	if (stored === null) return;
	try {
		const parsed = JSON.parse(stored) as { version: string; releaseUrl?: string };
		showUpdateAppliedNotification(parsed.version, parsed.releaseUrl);
	} catch (err) {
		console.warn('Failed to parse pending update-applied notice', err);
	}
};

const notifyYoutubeUploadChanged = (upload: import('./api').YouTubeUploadStatus): void => {
	if ((upload.state === 'queued' || upload.state === 'uploading') && !youtubeStartedNotificationIds.has(upload.id)) {
		const flag = addNotificationFlag({
			key: `youtube-upload-${upload.id}`,
			title: 'YouTube upload started',
			detail: upload.title || upload.fileName,
			tone: 'info',
			sticky: true
		});
		youtubeStartedNotificationIds.set(upload.id, flag.id);
	}

	if (upload.state === 'failed' && !youtubeNotifiedFailedIds.has(upload.id)) {
		youtubeNotifiedFailedIds.add(upload.id);
		const startedNotificationId = youtubeStartedNotificationIds.get(upload.id);
		const notification = {
			key: `youtube-upload-${upload.id}`,
			title: 'YouTube upload failed',
			detail: upload.title || upload.fileName,
			meta: 'An error occurred when trying to upload the video.',
			tone: 'error' as const,
			timeoutMs: 8000
		};
		if (startedNotificationId !== undefined) {
			replaceNotificationFlag(startedNotificationId, notification);
			youtubeStartedNotificationIds.delete(upload.id);
		} else {
			addNotificationFlag(notification);
		}
	}

	if (upload.state === 'uploaded' && upload.videoUrl && !youtubeNotifiedCompletedIds.has(upload.id)) {
		youtubeNotifiedCompletedIds.add(upload.id);
		const startedNotificationId = youtubeStartedNotificationIds.get(upload.id);
		const notification = {
			key: `youtube-upload-${upload.id}`,
			title: 'YouTube upload completed',
			detail: upload.title || upload.fileName,
			meta: 'Click to open YouTube.',
			tone: 'success' as const,
			timeoutMs: 8000,
			action: () => {
				void backend.openYouTubeUrl(upload.videoUrl!).catch((err) => console.warn('Failed to open YouTube video', err));
			}
		};
		if (startedNotificationId !== undefined) {
			replaceNotificationFlag(startedNotificationId, notification);
			youtubeStartedNotificationIds.delete(upload.id);
		} else {
			addNotificationFlag(notification);
		}
	}
};

const dismissSettingsError = (): void => {
	dismissNotificationFlagsByKey('settings-config-error');
	if (settingsErrorNotificationId !== null) {
		dismissNotificationFlag(settingsErrorNotificationId);
		settingsErrorNotificationId = null;
	}
};

const showSettingsError = (error: string): void => {
	const notification = {
		key: 'settings-config-error',
		title: 'Config file invalid',
		detail: error,
		meta: 'Click to open options.',
		tone: 'error' as const,
		sticky: true,
		href: '/options'
	};
	if (settingsErrorNotificationId !== null && replaceNotificationFlag(settingsErrorNotificationId, notification)) {
		return;
	}
	settingsErrorNotificationId = addNotificationFlag(notification).id;
};

const applyAppSnapshot = (snapshot: AppSnapshot): void => {
	applyMonitorSnapshot(snapshot);
	setObsSources(snapshot.sources);
	setReplayBufferStatus(snapshot.replayBuffer);
	if (!settings.dirty) {
		settings.applyStatus(snapshot.settingsStatus);
		if (settings.fileError) showSettingsError(settings.fileError);
		else dismissSettingsError();
	}
	if (snapshot.update) {
		applyUpdateAvailable(snapshot.update);
	} else {
		dismissUpdateAvailableNotification();
		dismissedUpdateVersion = null;
	}
};

const handleAppEvent = (event: AppEvent): void => {
	switch (event.type) {
		case 'version': {
			if (typeof event.buildId !== 'string') {
				console.warn('Ignoring malformed app version event', event);
				return;
			}
			const self = backend.selfBuildId();
			if (self !== null && self !== event.buildId) {
				console.warn(`frontend build ${self} differs from backend build ${event.buildId}; reloading`);
				window.location.reload();
			}
			break;
		}
		case 'snapshot':
			if (event.state && typeof event.state === 'object') applyAppSnapshot(event.state);
			else console.warn('Ignoring malformed snapshot event', event);
			break;
		case 'languageDetected':
			applyLanguageDetected(event.lang);
			break;
		case 'monitorFps':
			applyMonitorFps(event);
			break;
		case 'recordingSavePending':
			applyRecordingSavePending(event);
			break;
		case 'recordingSaved':
			applyRecordingSaved(event);
			break;
		case 'recordingSaveDiscarded':
			applyRecordingSaveDiscarded(event);
			break;
		case 'failedRunNotSaved':
			applyFailedRunNotSaved(event.reason);
			break;
		case 'monitorStopped':
			applyMonitorStopped(event.reason);
			void refreshReplayBuffer();
			break;
		case 'settingsReloaded':
			settings.applyReloaded(event.settings, event.configPath);
			dismissSettingsError();
			addNotificationFlag({ title: 'Config reloaded', detail: event.configPath, tone: 'success' });
			break;
		case 'settingsInvalid':
			settings.applyInvalid(event.error, event.configPath);
			showSettingsError(event.error);
			break;
		case 'updateApplied':
			if (typeof event.version === 'string') handleUpdateApplied(event.version, event.releaseUrl);
			else console.warn('Ignoring malformed updateApplied event', event);
			break;
		case 'updateStagingFailed':
			addNotificationFlag({
				key: 'plugin-update-staging-failed',
				title: 'Plugin update failed',
				detail: event.error,
				tone: 'error',
				sticky: true
			});
			break;
		case 'youtubeUploadChanged':
			youtube.applyUpload(event.upload);
			notifyYoutubeUploadChanged(event.upload);
			break;
		default:
			console.warn('Ignoring unknown app event', event);
	}
};

const dismissUpdateAvailableNotification = (): void => {
	if (updateNotificationId === null) return;
	removeNotificationFlag(updateNotificationId);
	updateNotificationId = null;
};

const applyUpdateAvailable = (update: PluginUpdate): void => {
	// With auto-update on, the plugin stages and reports back via the
	// "update found" / "plugin updated" notices, so a sticky "open the
	// release page" notice would just be noise suggesting a needless step.
	if (settings.autoUpdateEnabled) {
		dismissUpdateAvailableNotification();
		return;
	}
	if (dismissedUpdateVersion === update.latestVersion) return;
	const notification = updateNotification(update);
	if (updateNotificationId !== null && replaceNotificationFlag(updateNotificationId, notification)) {
		return;
	}
	updateNotificationId = addNotificationFlag(notification).id;
};

const clearReconnectTimer = (): void => {
	if (reconnectTimer !== null) {
		clearTimeout(reconnectTimer);
		reconnectTimer = null;
	}
};

const scheduleReconnect = (): void => {
	if (stopped || reconnectTimer !== null) return;
	reconnectTimer = setTimeout(() => {
		reconnectTimer = null;
		connect();
	}, 1000);
};

const connect = (): void => {
	if (!browser || stopped || socket !== null) return;

	const nextSocket = backend.connectAppSocket(handleAppEvent, () => {
		if (socket === nextSocket) socket = null;
		scheduleReconnect();
	});
	socket = nextSocket;
};

/** Downloads, verifies, and installs the notice's update, keeping one progress
 * flag updated throughout. Applying briefly drops the connection while the core
 * swaps in. */
const downloadAndInstall = async (update: PluginUpdate): Promise<void> => {
	const progressId = addNotificationFlag({
		key: 'plugin-update-installing',
		title: 'Installing update',
		detail: `Downloading and verifying ${update.latestVersion}...`,
		tone: 'info',
		sticky: true
	}).id;
	try {
		await backend.downloadUpdateNow();
		await backend.applyUpdateNow();
		replaceNotificationFlag(progressId, {
			key: 'plugin-update-installing',
			title: 'Applying update',
			detail: 'The plugin will briefly reconnect while the update is installed.',
			tone: 'success'
		});
	} catch (err) {
		replaceNotificationFlag(progressId, {
			key: 'plugin-update-installing',
			title: 'Update failed',
			detail: err instanceof Error ? err.message : String(err),
			tone: 'error',
			sticky: true
		});
	}
};

const updateNotification = (update: PluginUpdate) => ({
	key: 'plugin-update-available',
	title: 'Plugin update available',
	detail: `${update.currentVersion} -> ${update.latestVersion}`,
	meta: 'Click to download and install.',
	tone: 'info' as const,
	sticky: true,
	onDismiss: () => {
		if (updateNotificationId !== null) updateNotificationId = null;
		dismissedUpdateVersion = update.latestVersion;
	},
	action: async () => {
		dismissedUpdateVersion = update.latestVersion;
		dismissUpdateAvailableNotification();
		await downloadAndInstall(update);
	}
});

export const startAppSocket = (): void => {
	if (!browser) return;
	// Picks up a notice persisted by handleUpdateApplied just before a
	// production reload landed us here on the fresh page.
	consumePendingUpdateAppliedNotification();
	stopped = false;
	connect();
};

export const stopAppSocket = (): void => {
	stopped = true;
	clearReconnectTimer();
	socket?.close();
	socket = null;
};
