import { browser } from '$app/environment';
import { connectAppSocket } from './api';
import {
	applyLanguageDetected,
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
		onClose: () => {
			if (socket === nextSocket) socket = null;
			scheduleReconnect();
		}
	});
	socket = nextSocket;
};

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
