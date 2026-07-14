import {
	getMonitorStatus,
	type LevelMatch,
	type MonitorFps,
	type MonitorStatus,
	type MonitorStoppedReason,
	type RecordingSaveDiscarded,
	type RecordingSavePending,
	type RecordingSaved,
	type RecordingStatus
} from './api';
import { addNotificationFlag, dismissNotificationFlag, replaceNotificationFlag } from './notifications.svelte';

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

/** Shared, reactive monitor state. Refreshed by the root layout on navigation;
 * the live monitor socket keeps route remounts from losing recorder UI state. */
export const monitor = $state<{
	status: MonitorStatus | null;
	loaded: boolean;
	match: LevelMatch | null;
	fps: MonitorFps | null;
	recordingState: RecordingStatus | null;
	kiaEffectId: number;
}>({
	status: null,
	loaded: false,
	match: null,
	fps: null,
	recordingState: null,
	kiaEffectId: 0
});

export const monitorHref = (status: MonitorStatus | null = monitor.status): string | null => {
	if (!status?.enabled) return null;
	return `/sources/${encodeURIComponent(status.sourceName)}`;
};

const clearRunState = () => {
	monitor.match = null;
	monitor.fps = null;
	monitor.recordingState = null;
};

const pendingSaveNotificationIds = new Map<number, number>();
let languageDetectedNotificationId: number | null = null;

const visibleRecordingState = (status: RecordingStatus | null): RecordingStatus | null =>
	status === 'savePending' ? null : status;

const formatWholeSeconds = (value: number): string => {
	const seconds = Math.max(0, Math.ceil(value));
	return `${seconds} second${seconds === 1 ? '' : 's'}`;
};

const formatRunTime = (seconds: number): string => {
	const m = Math.floor(seconds / 60);
	const s = seconds % 60;
	return `${m}:${s.toString().padStart(2, '0')}`;
};

const savePendingDetail = (pending: RecordingSavePending): string => {
	const parts = [
		pending.levelNumber ? `${pending.levelNumber}. ${pending.level}` : pending.level,
		pending.difficulty,
		pending.timeSecs !== undefined ? formatRunTime(pending.timeSecs) : undefined,
		pending.status
	].filter((part): part is string => Boolean(part));
	return parts.join(' | ');
};

const savePendingMeta = (pending: RecordingSavePending): string => {
	const parts = [`Clip will save in ${formatWholeSeconds(pending.saveInSecs)}`];
	parts.push(`about ${pending.estimatedDurationSecs.toFixed(1)}s`);
	if (pending.targetTimeSecs !== undefined) parts.push(`target ${formatRunTime(pending.targetTimeSecs)}`);
	if (pending.bestTimeSecs !== undefined) parts.push(`best ${formatRunTime(pending.bestTimeSecs)}`);
	return parts.join(' | ');
};

export const applyMonitorMatch = (match: LevelMatch): void => {
	monitor.match = match;
};

export const applyRecordingState = (status: RecordingStatus | null): void => {
	const previous = monitor.recordingState;
	monitor.recordingState = visibleRecordingState(status);
	if (status === 'kia' && previous !== 'kia') {
		triggerKiaDeathOverlay();
	}
};

const languageLabel = (lang: string): string => {
	switch (lang) {
		case 'en':
			return 'English';
		case 'jp':
			return 'Japanese';
		default:
			return lang.toUpperCase();
	}
};

export const applyLanguageDetected = (lang: 'en' | 'jp'): void => {
	const notification = {
		title: 'ROM language detected',
		detail: `${languageLabel(lang)} templates are active for this source.`,
		meta: 'Monitoring will switch automatically if needed.',
		tone: 'info',
		sticky: false
	} as const;

	if (languageDetectedNotificationId !== null) {
		const replaced = replaceNotificationFlag(languageDetectedNotificationId, notification);
		if (replaced) return;
	}
	languageDetectedNotificationId = addNotificationFlag(notification).id;
};

export const applyMonitorFps = (fps: MonitorFps): void => {
	monitor.fps = fps;
};

export const triggerKiaDeathOverlay = (): void => {
	monitor.kiaEffectId += 1;
};

export const applyRecordingSavePending = (pending: RecordingSavePending): void => {
	const notification = {
		title: 'Saving recording',
		detail: savePendingDetail(pending),
		meta: savePendingMeta(pending),
		tone: pending.failed ? 'warning' : 'info',
		sticky: true
	} as const;

	// The backend re-sends this under the same saveId when the run time is
	// refined; replace the existing toast in place so a corrected time doesn't
	// leave the first (stale) one stuck alongside it.
	const existingFlagId = pendingSaveNotificationIds.get(pending.saveId);
	if (existingFlagId !== undefined && replaceNotificationFlag(existingFlagId, notification)) {
		return;
	}
	const flag = addNotificationFlag(notification);
	pendingSaveNotificationIds.set(pending.saveId, flag.id);
};

export const applyRecordingSaveDiscarded = (discarded: RecordingSaveDiscarded): void => {
	const flagId = pendingSaveNotificationIds.get(discarded.saveId);
	if (flagId === undefined) return;
	pendingSaveNotificationIds.delete(discarded.saveId);
	dismissNotificationFlag(flagId);
};

export const applyRecordingSaved = (saved: RecordingSaved): void => {
	if (
		monitor.recordingState === 'complete' ||
		monitor.recordingState === 'failed' ||
		monitor.recordingState === 'aborted' ||
		monitor.recordingState === 'kia' ||
		monitor.recordingState === 'statsSkipped'
	) {
		monitor.recordingState = null;
	}
	const notification = {
		title: 'Clip saved',
		detail: saved.path,
		meta: `${saved.durationSecs.toFixed(1)}s${saved.failed ? ' - failed' : ''}`,
		tone: saved.failed ? 'warning' : 'success'
	} as const;
	const pendingFlagId = pendingSaveNotificationIds.get(saved.saveId);
	if (pendingFlagId !== undefined) {
		pendingSaveNotificationIds.delete(saved.saveId);
		if (replaceNotificationFlag(pendingFlagId, notification)) return;
	}
	addNotificationFlag(notification);
};

export const setMonitorRunning = (sourceName: string): void => {
	clearRunState();
	monitor.status = { enabled: true, sourceName, recordingState: null };
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
			status.enabled && (!monitor.status?.enabled || monitor.status.sourceName !== status.sourceName);

		monitor.status = status;
		if (!status.enabled || monitorChanged) {
			clearRunState();
		}
		monitor.recordingState = status.enabled ? visibleRecordingState(status.recordingState) : null;
		return monitor.status;
	} catch (err) {
		monitor.status = null;
		throw err;
	} finally {
		monitor.loaded = true;
	}
};
