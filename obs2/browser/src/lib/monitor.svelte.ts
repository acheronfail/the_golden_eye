import {
	type AppSnapshot,
	type FailedRunNotSavedReason,
	type LevelMatch,
	type MonitorFps,
	type MonitorStatus,
	type MonitorStoppedReason,
	type RecordingSaveDiscarded,
	type SingleSegmentSnapshot,
	type RecordingSavePending,
	type RecordingSaved,
	type RecordingStatus
} from './api';
import { addNotificationFlag, dismissNotificationFlag, replaceNotificationFlag } from './notifications.svelte';
import { runModePath } from './singleSegment';

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
	singleSegment: SingleSegmentSnapshot;
	kiaEffectId: number;
}>({
	status: null,
	loaded: false,
	match: null,
	fps: null,
	recordingState: null,
	singleSegment: { started: false, splits: [] },
	kiaEffectId: 0
});

export const monitorHref = (status: MonitorStatus | null = monitor.status): string | null => {
	if (!status?.enabled) return null;
	return `/sources/${encodeURIComponent(status.sourceName)}/${runModePath(status.mode)}`;
};

const clearRunState = () => {
	monitor.match = null;
	monitor.fps = null;
	monitor.recordingState = null;
	monitor.singleSegment = { started: false, splits: [] };
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

export const monitorStatusFromSnapshot = (snapshot: AppSnapshot): MonitorStatus =>
	snapshot.monitor.enabled && snapshot.monitor.sourceName
		? {
				enabled: true,
				sourceName: snapshot.monitor.sourceName,
				mode: snapshot.monitor.mode ?? 'clips',
				recordingState: snapshot.recordingState
			}
		: { enabled: false, mode: 'clips', recordingState: null };

export const applyMonitorSnapshot = (snapshot: AppSnapshot): void => {
	const nextStatus = monitorStatusFromSnapshot(snapshot);
	const previousSource = monitor.status?.enabled ? monitor.status.sourceName : null;
	const nextSource = nextStatus.enabled ? nextStatus.sourceName : null;
	monitor.status = nextStatus;
	monitor.loaded = true;
	monitor.match = snapshot.match;
	monitor.recordingState = nextStatus.enabled ? visibleRecordingState(snapshot.recordingState) : null;
	monitor.singleSegment = snapshot.singleSegment ?? { started: false, splits: [] };
	if (!nextStatus.enabled || previousSource !== nextSource) {
		monitor.fps = null;
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

const failedRunNotSavedDetail = (reason: FailedRunNotSavedReason): string => {
	switch (reason) {
		case 'savingDisabled':
			return 'Saving failed runs is turned off in options.';
		case 'tooShort':
			return 'The run was shorter than the minimum failed-run length.';
	}
};

/** A failed run reached an ending but wasn't saved. Shown as a transient
 * notification -- never as a recorder phase, so a late-firing discard from an
 * earlier run can't knock a newly-started run out of its "recording" state. */
export const applyFailedRunNotSaved = (reason: FailedRunNotSavedReason): void => {
	addNotificationFlag({
		title: 'Failed run not saved',
		detail: failedRunNotSavedDetail(reason),
		tone: 'info',
		sticky: false
	});
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

export const applyMonitorStopped = (reason: MonitorStoppedReason): void => {
	clearRunState();
	monitor.status = { enabled: false, mode: 'clips', recordingState: null };
	monitor.loaded = true;
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
