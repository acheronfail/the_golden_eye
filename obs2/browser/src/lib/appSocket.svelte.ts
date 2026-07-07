import { browser } from '$app/environment';
import { connectAppSocket } from './api';
import {
	applyLanguageMismatch,
	applyMonitorMatch,
	applyMonitorStopped,
	applyRecordingSaved,
	applyRecordingSavePending,
	applyRecordingState
} from './monitor.svelte';
import { refreshReplayBuffer } from './replayBuffer.svelte';
import { setObsSources } from './sources.svelte';

let socket: WebSocket | null = null;
let reconnectTimer: ReturnType<typeof setTimeout> | null = null;
let stopped = true;

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
		onLanguageMismatch: applyLanguageMismatch,
		onRecordingSavePending: applyRecordingSavePending,
		onRecordingSaved: applyRecordingSaved,
		onMonitorStopped: (reason) => {
			applyMonitorStopped(reason);
			void refreshReplayBuffer();
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
