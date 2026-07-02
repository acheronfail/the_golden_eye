import { browser } from '$app/environment';
import {
	connectMonitorSocket,
	getMonitorStatus,
	type LevelMatch,
	type MonitorStatus,
	type RecordingSaved,
	type RecordingStatus
} from './api';

export interface MonitorPhaseStyle {
	title: string;
	border: string;
	heading: string;
	tag: string;
	button: string;
	dot: string;
}

export const monitorPhaseStyle = (state: RecordingStatus | null): MonitorPhaseStyle => {
	switch (state) {
		case 'started':
			return {
				title: 'recording',
				border: 'border-green-500',
				heading: 'text-green-300',
				tag: 'text-green-500',
				button: 'border-green-500 text-green-300 hover:bg-green-500 focus-visible:outline-green-400',
				dot: 'bg-green-400'
			};
		case 'cancelled':
			return {
				title: 'cancelled',
				border: 'border-neutral-500',
				heading: 'text-neutral-300',
				tag: 'text-neutral-500',
				button: 'border-neutral-500 text-neutral-300 hover:bg-neutral-500 focus-visible:outline-neutral-400',
				dot: 'bg-neutral-400'
			};
		case 'failed':
			return {
				title: 'failed',
				border: 'border-red-500',
				heading: 'text-red-300',
				tag: 'text-red-500',
				button: 'border-red-500 text-red-300 hover:bg-red-500 focus-visible:outline-red-400',
				dot: 'bg-red-400'
			};
		case 'aborted':
			return {
				title: 'aborted',
				border: 'border-red-500',
				heading: 'text-red-300',
				tag: 'text-red-500',
				button: 'border-red-500 text-red-300 hover:bg-red-500 focus-visible:outline-red-400',
				dot: 'bg-red-400'
			};
		case 'kia':
			return {
				title: 'killed in action',
				border: 'border-red-500',
				heading: 'text-red-300',
				tag: 'text-red-500',
				button: 'border-red-500 text-red-300 hover:bg-red-500 focus-visible:outline-red-400',
				dot: 'bg-red-400'
			};
		case 'complete':
			return {
				title: 'complete',
				border: 'border-fuchsia-500',
				heading: 'text-fuchsia-300',
				tag: 'text-fuchsia-500',
				button: 'border-fuchsia-500 text-fuchsia-300 hover:bg-fuchsia-500 focus-visible:outline-fuchsia-400',
				dot: 'bg-fuchsia-400'
			};
		case 'statsSkipped':
			return {
				title: 'skipped stats',
				border: 'border-red-500',
				heading: 'text-red-300',
				tag: 'text-red-500',
				button: 'border-red-500 text-red-300 hover:bg-red-500 focus-visible:outline-red-400',
				dot: 'bg-red-400'
			};
		case 'failedDiscarded':
			return {
				title: 'failed run not saved',
				border: 'border-neutral-500',
				heading: 'text-neutral-300',
				tag: 'text-neutral-500',
				button: 'border-neutral-500 text-neutral-300 hover:bg-neutral-500 focus-visible:outline-neutral-400',
				dot: 'bg-neutral-400'
			};
		case 'savePending':
			return {
				title: 'saving recording',
				border: 'border-cyan-500',
				heading: 'text-cyan-300',
				tag: 'text-cyan-500',
				button: 'border-cyan-500 text-cyan-300 hover:bg-cyan-500 focus-visible:outline-cyan-400',
				dot: 'bg-cyan-400'
			};
		case null:
		default:
			return {
				title: 'waiting for level start',
				border: 'border-amber-500',
				heading: 'text-amber-300',
				tag: 'text-amber-500',
				button: 'border-amber-500 text-amber-300 hover:bg-amber-500 focus-visible:outline-amber-400',
				dot: 'bg-amber-400'
			};
	}
};

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

let socket: WebSocket | null = null;

export const monitorHref = (status: MonitorStatus | null = monitor.status): string | null => {
	if (!status?.enabled) return null;
	return `/source/${encodeURIComponent(status.sourceName)}/${encodeURIComponent(status.lang)}`;
};

const clearRunState = () => {
	monitor.match = null;
	monitor.recordingState = null;
	monitor.lastSaved = null;
};

const applyRecordingState = (status: RecordingStatus | null): void => {
	monitor.recordingState = status;
};

const applyRecordingSaved = (saved: RecordingSaved): void => {
	monitor.lastSaved = saved;
	if (monitor.recordingState === 'savePending' || monitor.recordingState === 'statsSkipped') {
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
	monitor.status = { enabled: true, sourceName, lang, recordingState: null };
	monitor.loaded = true;
	syncSocket();
};

export const setMonitorStopped = (): void => {
	clearRunState();
	monitor.status = { enabled: false, recordingState: null };
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
		if (!status.enabled || monitorChanged) {
			clearRunState();
		}
		monitor.recordingState = status.enabled ? status.recordingState : null;
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
