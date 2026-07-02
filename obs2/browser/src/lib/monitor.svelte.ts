import { browser } from '$app/environment';
import {
	connectMonitorSocket,
	getMonitorStatus,
	type LevelMatch,
	type MonitorStatus,
	type RecordingSaved,
	type RecordingStatus
} from './api';

/**
 * Shared, reactive monitor state. The root layout refreshes this on navigation
 * so global UI can show when monitoring is active, while the live monitor socket
 * keeps route remounts from losing the recorder UI state.
 */
export const monitor = $state<{
	status: MonitorStatus | null;
	loaded: boolean;
	match: LevelMatch | null;
	recordingState: RecordingStatus | null;
	lastSaved: RecordingSaved | null;
}>({
	status: null,
	loaded: false,
	match: null,
	recordingState: null,
	lastSaved: null
});

const CANCELLED_LINGER_MS = 2000;
const SAVE_TIMEOUT_MS = 30000;

let socket: WebSocket | null = null;
let revertTimer: ReturnType<typeof setTimeout> | null = null;

export const monitorHref = (status: MonitorStatus | null = monitor.status): string | null => {
	if (!status?.enabled) return null;
	return `/source/${encodeURIComponent(status.sourceName)}/${encodeURIComponent(status.lang)}`;
};

const clearRevertTimer = () => {
	if (revertTimer !== null) {
		clearTimeout(revertTimer);
		revertTimer = null;
	}
};

const clearRunState = () => {
	clearRevertTimer();
	monitor.match = null;
	monitor.recordingState = null;
	monitor.lastSaved = null;
};

const applyRecordingState = (status: RecordingStatus): void => {
	// A fresh transition always supersedes a pending revert-to-idle timer.
	clearRevertTimer();
	monitor.recordingState = status;
	if (status === 'cancelled' || status === 'failedDiscarded') {
		revertTimer = setTimeout(() => {
			monitor.recordingState = null;
			revertTimer = null;
		}, CANCELLED_LINGER_MS);
	} else if (status === 'savePending' || status === 'statsSkipped') {
		// Normally `recordingSaved` clears us back to idle; this is the fallback
		// if that event never lands so we don't sit on "saving" forever.
		revertTimer = setTimeout(() => {
			monitor.recordingState = null;
			revertTimer = null;
		}, SAVE_TIMEOUT_MS);
	}
};

const applyRecordingSaved = (saved: RecordingSaved): void => {
	monitor.lastSaved = saved;
	if (monitor.recordingState === 'savePending' || monitor.recordingState === 'statsSkipped') {
		clearRevertTimer();
		monitor.recordingState = null;
	}
};

const connectSocket = (): void => {
	if (!browser || socket !== null) return;
	const nextSocket = connectMonitorSocket({
		onMatch: (match) => {
			monitor.match = match;
		},
		onRecordingState: applyRecordingState,
		onRecordingSaved: applyRecordingSaved,
		onClose: () => {
			if (socket === nextSocket) socket = null;
		}
	});
	socket = nextSocket;
};

const disconnectSocket = (): void => {
	socket?.close();
	socket = null;
};

const syncSocket = (): void => {
	if (monitor.status?.enabled) {
		connectSocket();
	} else {
		disconnectSocket();
	}
};

export const setMonitorRunning = (sourceName: string, lang: string): void => {
	clearRunState();
	monitor.status = { enabled: true, sourceName, lang };
	monitor.loaded = true;
	syncSocket();
};

export const setMonitorStopped = (): void => {
	clearRunState();
	monitor.status = { enabled: false };
	monitor.loaded = true;
	syncSocket();
};

/** Re-query the backend for the current monitor status. */
export const refreshMonitor = async (): Promise<MonitorStatus> => {
	try {
		const status = await getMonitorStatus();
		const monitorChanged =
			status.enabled &&
			(!monitor.status?.enabled ||
				monitor.status.sourceName !== status.sourceName ||
				monitor.status.lang !== status.lang);

		monitor.status = status;
		if (!status.enabled || monitorChanged) clearRunState();
		syncSocket();
		return monitor.status;
	} catch (err) {
		monitor.status = null;
		syncSocket();
		throw err;
	} finally {
		monitor.loaded = true;
	}
};
