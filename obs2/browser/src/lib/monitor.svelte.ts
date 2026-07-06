import {
	getMonitorStatus,
	type LevelMatch,
	type MonitorStatus,
	type MonitorStoppedReason,
	type RecordingSaved,
	type RecordingStatus
} from './api';
import { addNotificationFlag } from './notifications.svelte';

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
				border: 'obs-phase-recording-border',
				heading: 'obs-phase-recording-text',
				tag: 'obs-phase-recording-text',
				button: 'obs-phase-recording-button',
				dot: 'obs-phase-recording-dot'
			};
		case 'cancelled':
			return {
				title: 'cancelled',
				border: 'obs-phase-neutral-border',
				heading: 'obs-phase-neutral-text',
				tag: 'obs-phase-neutral-text',
				button: 'obs-phase-neutral-button',
				dot: 'obs-phase-neutral-dot'
			};
		case 'failed':
			return {
				title: 'failed',
				border: 'obs-phase-danger-border',
				heading: 'obs-phase-danger-text',
				tag: 'obs-phase-danger-text',
				button: 'obs-phase-danger-button',
				dot: 'obs-phase-danger-dot'
			};
		case 'aborted':
			return {
				title: 'aborted',
				border: 'obs-phase-danger-border',
				heading: 'obs-phase-danger-text',
				tag: 'obs-phase-danger-text',
				button: 'obs-phase-danger-button',
				dot: 'obs-phase-danger-dot'
			};
		case 'kia':
			return {
				title: 'killed in action',
				border: 'obs-phase-danger-border',
				heading: 'obs-phase-danger-text',
				tag: 'obs-phase-danger-text',
				button: 'obs-phase-danger-button',
				dot: 'obs-phase-danger-dot'
			};
		case 'complete':
			return {
				title: 'complete',
				border: 'obs-phase-gold-border',
				heading: 'obs-phase-gold-text',
				tag: 'obs-phase-gold-text',
				button: 'obs-phase-gold-button',
				dot: 'obs-phase-gold-dot'
			};
		case 'statsSkipped':
			return {
				title: 'skipped stats',
				border: 'obs-phase-danger-border',
				heading: 'obs-phase-danger-text',
				tag: 'obs-phase-danger-text',
				button: 'obs-phase-danger-button',
				dot: 'obs-phase-danger-dot'
			};
		case 'failedDiscarded':
			return {
				title: 'failed run not saved',
				border: 'obs-phase-neutral-border',
				heading: 'obs-phase-neutral-text',
				tag: 'obs-phase-neutral-text',
				button: 'obs-phase-neutral-button',
				dot: 'obs-phase-neutral-dot'
			};
		case 'savePending':
			return {
				title: 'saving recording',
				border: 'obs-phase-gold-border',
				heading: 'obs-phase-gold-text',
				tag: 'obs-phase-gold-text',
				button: 'obs-phase-gold-button',
				dot: 'obs-phase-gold-dot'
			};
		case null:
		default:
			return {
				title: 'waiting',
				border: 'obs-phase-gold-border',
				heading: 'obs-phase-gold-text',
				tag: 'obs-phase-gold-text',
				button: 'obs-phase-gold-button',
				dot: 'obs-phase-gold-dot'
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
	kiaEffectId: number;
}>({
	status: null,
	loaded: false,
	match: null,
	recordingState: null,
	kiaEffectId: 0
});

export const monitorHref = (status: MonitorStatus | null = monitor.status): string | null => {
	if (!status?.enabled) return null;
	return `/source/${encodeURIComponent(status.sourceName)}/${encodeURIComponent(status.lang)}`;
};

const clearRunState = () => {
	monitor.match = null;
	monitor.recordingState = null;
};

export const applyMonitorMatch = (match: LevelMatch): void => {
	monitor.match = match;
};

export const applyRecordingState = (status: RecordingStatus | null): void => {
	const previous = monitor.recordingState;
	monitor.recordingState = status;
	if (status === 'kia' && previous !== 'kia') {
		triggerKiaDeathOverlay();
	}
};

export const triggerKiaDeathOverlay = (): void => {
	monitor.kiaEffectId += 1;
};

export const applyRecordingSaved = (saved: RecordingSaved): void => {
	if (monitor.recordingState === 'savePending' || monitor.recordingState === 'statsSkipped') {
		monitor.recordingState = null;
	}
	addNotificationFlag({
		title: 'Clip saved',
		detail: saved.path,
		meta: `${saved.durationSecs.toFixed(1)}s${saved.failed ? ' - failed' : ''}`,
		tone: saved.failed ? 'warning' : 'success'
	});
};

export const setMonitorRunning = (sourceName: string, lang: string): void => {
	clearRunState();
	monitor.status = { enabled: true, sourceName, lang, recordingState: null };
	monitor.loaded = true;
};

export const setMonitorStopped = (): void => {
	clearRunState();
	monitor.status = { enabled: false, recordingState: null };
	monitor.loaded = true;
};

export const applyMonitorStopped = (reason: MonitorStoppedReason): void => {
	setMonitorStopped();
	if (reason === 'replayBufferStopped') {
		addNotificationFlag({
			title: 'Monitoring disabled',
			detail: "OBS's replay buffer was unexpectedly stopped.",
			meta: 'Monitoring was disabled because clips can no longer be saved.',
			tone: 'error',
			sticky: true
		});
	}
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
		return monitor.status;
	} catch (err) {
		monitor.status = null;
		throw err;
	} finally {
		monitor.loaded = true;
	}
};
