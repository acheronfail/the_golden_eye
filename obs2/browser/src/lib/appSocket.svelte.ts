import { browser } from '$app/environment';
import { connectAppSocket, openUpdateRelease, type PluginUpdate } from './api';
import {
	applyLanguageDetected,
	applyMonitorFps,
	applyMonitorMatch,
	applyMonitorStopped,
	applyRecordingSaved,
	applyRecordingSavePending,
	applyRecordingState
} from './monitor.svelte';
import {
	addNotificationFlag,
	dismissNotificationFlag,
	dismissNotificationFlagsByKey,
	replaceNotificationFlag
} from './notifications.svelte';
import { refreshReplayBuffer } from './replayBuffer.svelte';
import { settings } from './settings.svelte';
import { setObsSources } from './sources.svelte';

let socket: WebSocket | null = null;
let reconnectTimer: ReturnType<typeof setTimeout> | null = null;
let stopped = true;
let settingsErrorNotificationId: number | null = null;
let updateNotificationId: number | null = null;

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

	const nextSocket = connectAppSocket({
		onSources: setObsSources,
		onMatch: applyMonitorMatch,
		onRecordingState: applyRecordingState,
		onLanguageDetected: applyLanguageDetected,
		onMonitorFps: applyMonitorFps,
		onRecordingSavePending: applyRecordingSavePending,
		onRecordingSaved: applyRecordingSaved,
		onMonitorStopped: (reason) => {
			applyMonitorStopped(reason);
			void refreshReplayBuffer();
		},
		onSettingsReloaded: (nextSettings, configPath) => {
			settings.applyReloaded(nextSettings, configPath);
			dismissNotificationFlagsByKey('settings-config-error');
			if (settingsErrorNotificationId !== null) {
				dismissNotificationFlag(settingsErrorNotificationId);
				settingsErrorNotificationId = null;
			}
			addNotificationFlag({
				title: 'Config reloaded',
				detail: configPath,
				tone: 'success'
			});
		},
		onSettingsInvalid: (error, configPath) => {
			settings.applyInvalid(error, configPath);
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
		},
		onUpdateAvailable: (update) => {
			const notification = updateNotification(update);
			if (updateNotificationId !== null && replaceNotificationFlag(updateNotificationId, notification)) {
				return;
			}
			updateNotificationId = addNotificationFlag(notification).id;
		},
		onClose: () => {
			if (socket === nextSocket) socket = null;
			scheduleReconnect();
		}
	});
	socket = nextSocket;
};

const updateNotification = (update: PluginUpdate) => ({
	key: 'plugin-update-available',
	title: 'Plugin update available',
	detail: `${update.currentVersion} -> ${update.latestVersion}`,
	meta: 'Click to open the latest GitHub release.',
	tone: 'info' as const,
	sticky: true,
	action: async () => {
		try {
			await openUpdateRelease(update.releaseUrl);
		} catch (err) {
			console.warn('Failed to open plugin update release', err);
		}
	}
});

export const startAppSocket = (): void => {
	if (!browser) return;
	stopped = false;
	connect();
};

export const stopAppSocket = (): void => {
	stopped = true;
	clearReconnectTimer();
	socket?.close();
	socket = null;
};
