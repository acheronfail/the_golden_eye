import { browser } from '$app/environment';
import { backend, type AppEvent, type AppSnapshot } from '$lib/api';
import {
	applyMonitorFps,
	applyMonitorSnapshot,
	applyMonitorStopped,
	applyRecordingSaved
} from '$lib/stores/monitor.svelte';

import { refreshReplayBuffer, setReplayBufferStatus } from '$lib/stores/replayBuffer.svelte';
import { settings } from '$lib/stores/settings.svelte';
import { recentRuns } from '$lib/stores/recentRuns.svelte';
import { setRunCatalogSync } from '$lib/stores/runCatalog.svelte';
import { setObsSources } from '$lib/stores/sources.svelte';
import { updates } from '$lib/stores/updates.svelte';
import { youtube } from '$lib/stores/youtube.svelte';

let socket: WebSocket | null = null;
let reconnectTimer: ReturnType<typeof setTimeout> | null = null;
let stopped = true;
const applyAppSnapshot = (snapshot: AppSnapshot): void => {
	applyMonitorSnapshot(snapshot);
	setRunCatalogSync(snapshot.runCatalogSync ?? null);
	setObsSources(snapshot.sources);
	setReplayBufferStatus(snapshot.replayBuffer);
	settings.handleSnapshot(snapshot.settingsStatus);
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
		case 'monitorFps':
			applyMonitorFps(event);
			break;
		case 'recordingSavePending':
			recentRuns.applySavePending(event);
			break;
		case 'recordingSaved':
			applyRecordingSaved(event);
			void recentRuns.refresh();
			break;
		case 'runCatalogChanged':
			void recentRuns.refresh(event.saveId);
			break;
		case 'monitorStopped':
			applyMonitorStopped(event.reason);
			void refreshReplayBuffer();
			break;
		case 'settingsReloaded':
			settings.handleReloaded(event.settings, event.configPath);
			break;
		case 'settingsInvalid':
			settings.handleInvalid(event.error, event.configPath);
			break;
		case 'updateApplied':
			if (typeof event.version === 'string') updates.handleApplied(event.version, event.releaseUrl);
			else console.warn('Ignoring malformed updateApplied event', event);
			break;
		case 'updateStagingFailed':
			updates.handleStagingFailed(event.error);
			break;
		case 'youtubeUploadChanged':
			youtube.handleUploadChanged(event.upload);
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
	stopped = false;
	connect();
};

export const stopAppSocket = (): void => {
	stopped = true;
	clearReconnectTimer();
	socket?.close();
	socket = null;
};
