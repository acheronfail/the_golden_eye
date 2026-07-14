import { browser } from '$app/environment';
import {
	applyUpdateNow,
	connectAppSocket,
	downloadUpdateNow,
	openUpdateRelease,
	selfBuildId,
	type PluginUpdate
} from './api';
import {
	applyLanguageDetected,
	applyMonitorFps,
	applyMonitorMatch,
	applyMonitorStopped,
	applyRecordingSaved,
	applyRecordingSaveDiscarded,
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
						await openUpdateRelease(releaseUrl);
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
	if (selfBuildId() === null) {
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
		onRecordingSaveDiscarded: applyRecordingSaveDiscarded,
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
			// With auto-update on, the plugin stages and reports back via the
			// "update found" / "plugin updated" notices, so a sticky "open the
			// release page" notice would just be noise suggesting a needless step.
			if (settings.autoUpdateEnabled) {
				if (updateNotificationId !== null) {
					dismissNotificationFlag(updateNotificationId);
					updateNotificationId = null;
				}
				return;
			}
			const notification = updateNotification(update);
			if (updateNotificationId !== null && replaceNotificationFlag(updateNotificationId, notification)) {
				return;
			}
			updateNotificationId = addNotificationFlag(notification).id;
		},
		onUpdateApplied: handleUpdateApplied,
		onUpdateStagingFailed: (error) => {
			addNotificationFlag({
				key: 'plugin-update-staging-failed',
				title: 'Plugin update failed',
				detail: error,
				tone: 'error',
				sticky: true
			});
		},
		onClose: () => {
			if (socket === nextSocket) socket = null;
			scheduleReconnect();
		}
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
		await downloadUpdateNow();
		await applyUpdateNow();
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
	action: async () => {
		if (updateNotificationId !== null) {
			dismissNotificationFlag(updateNotificationId);
			updateNotificationId = null;
		}
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
