import { browser } from '$app/environment';
import { backend, type AppEvent, type AppSnapshot } from '$lib/api';
import {
	applyFailedRunNotSaved,
	applyLanguageDetected,
	applyMonitorFps,
	applyMonitorSnapshot,
	applyMonitorStopped,
	applyRecordingSaved,
	applyRecordingSaveDiscarded,
	applyRecordingSavePending
} from '$lib/stores/monitor.svelte';
import {
	addNotificationFlag,
	dismissNotificationFlag,
	dismissNotificationFlagsByKey,
	replaceNotificationFlag
} from '$lib/stores/notifications.svelte';
import { refreshReplayBuffer, setReplayBufferStatus } from '$lib/stores/replayBuffer.svelte';
import { settings } from '$lib/stores/settings.svelte';
import { setObsSources } from '$lib/stores/sources.svelte';
import { updates } from '$lib/stores/updates.svelte';
import { youtube } from '$lib/stores/youtube.svelte';

let socket: WebSocket | null = null;
let reconnectTimer: ReturnType<typeof setTimeout> | null = null;
let stopped = true;
let settingsErrorNotificationId: number | null = null;
const youtubeStartedNotificationIds = new Map<string, number>();
const youtubeNotifiedCompletedIds = new Set<string>();
const youtubeNotifiedFailedIds = new Set<string>();
const UPDATE_APPLIED_STORAGE_KEY = 'ge-update-applied-version';

const discardPersistedUpdateAppliedNotification = (): void => {
	try {
		sessionStorage.removeItem(UPDATE_APPLIED_STORAGE_KEY);
	} catch (err) {
		console.warn('Failed to discard legacy update-applied notice', err);
	}
};

const notifyYoutubeUploadChanged = (upload: import('$lib/api').YouTubeUploadStatus): void => {
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
	updates.applyStatus(snapshot.update);
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
			if (typeof event.version === 'string') updates.handleApplied(event.version, event.releaseUrl);
			else console.warn('Ignoring malformed updateApplied event', event);
			break;
		case 'updateStagingFailed':
			updates.handleStagingFailed(event.error);
			break;
		case 'youtubeUploadChanged':
			youtube.applyUpload(event.upload);
			notifyYoutubeUploadChanged(event.upload);
			break;
		case 'youtubeStatusChanged':
			youtube.applyStatus(event.status);
			break;
		default:
			console.warn('Ignoring unknown app event', event);
	}
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

export const startAppSocket = (): void => {
	if (!browser) return;
	discardPersistedUpdateAppliedNotification();
	stopped = false;
	connect();
};

export const stopAppSocket = (): void => {
	stopped = true;
	clearReconnectTimer();
	socket?.close();
	socket = null;
};
