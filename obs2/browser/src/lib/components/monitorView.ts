import type { LevelMatch, MonitorFps, RecordingStatus } from '$lib/api';
import { monitorPhaseStyle } from '$lib/stores/monitor.svelte';

export type MonitorTransition = 'starting' | 'stopping' | null;
export type MonitorDesign = 'signal-band' | 'mission-glass';
export type MonitorPhase = 'waiting' | 'recording' | 'complete' | 'danger' | 'neutral';

export interface MonitorViewProps {
	verified: boolean;
	monitoring: boolean;
	transition?: MonitorTransition;
	recordingState?: RecordingStatus | null;
	match?: LevelMatch | null;
	fps?: MonitorFps | null;
	showMonitorFps?: boolean;
	onStop: () => void;
}

export interface MonitorPresentation {
	waitingForObs: boolean;
	statusLabel: string;
	title: string;
	detail: string;
	showDetail: boolean;
	phase: MonitorPhase;
	animationKey: string;
	fpsText: string | null;
	fpsLagging: boolean;
}

const phaseFor = (state: RecordingStatus | null, waitingForObs: boolean): MonitorPhase => {
	if (waitingForObs) return 'neutral';
	switch (state) {
		case 'started':
			return 'recording';
		case 'complete':
			return 'complete';
		case 'failed':
		case 'aborted':
		case 'kia':
		case 'statsSkipped':
			return 'danger';
		case 'cancelled':
			return 'neutral';
		default:
			return 'waiting';
	}
};

export const monitorPresentation = ({
	verified,
	transition = null,
	recordingState = null,
	match = null,
	fps = null
}: MonitorViewProps): MonitorPresentation => {
	const waitingForObs = transition !== null;
	const title = waitingForObs ? 'waiting for OBS' : monitorPhaseStyle(recordingState).title;
	const detail =
		transition === 'starting'
			? 'replay buffer is stopping or starting'
			: transition === 'stopping'
				? 'stopping monitor'
				: (match?.screen ?? '...');
	const fpsText = fps
		? fps.sourceFps > 0
			? `${fps.processedFps.toFixed(1)} / ${fps.sourceFps.toFixed(1)} FPS`
			: `${fps.processedFps.toFixed(1)} FPS`
		: null;

	return {
		waitingForObs,
		statusLabel:
			transition === 'starting' ? 'Starting monitor' : transition === 'stopping' ? 'Stopping monitor' : 'Monitoring',
		title,
		detail,
		showDetail: waitingForObs || detail.trim().toLowerCase() !== 'unknown',
		phase: verified ? phaseFor(recordingState, waitingForObs) : 'neutral',
		animationKey: [
			verified ? 'verified' : 'unverified',
			transition ? `transition-${transition}` : (recordingState ?? 'waiting')
		].join(':'),
		fpsText,
		fpsLagging: Boolean(fps && fps.sourceFps > 0 && fps.processedFps + 0.5 < fps.sourceFps)
	};
};

export const formatMonitorTime = (secs: number): string => {
	const m = Math.floor(secs / 60);
	const s = secs % 60;
	return `${m}:${s.toString().padStart(2, '0')}`;
};
