import type { LevelMatch, MonitorFps, RecordingStatus, ReplaySaveStatus, RunClip } from '$lib/api';
import { monitorPhaseStyle, monitorPresentationPhase, type MonitorPhase } from '$lib/stores/monitor.svelte';

export type MonitorTransition = 'starting' | 'stopping' | null;
export type MonitorDesign = 'signal-band' | 'mission-glass' | 'debug';
export type { MonitorPhase } from '$lib/stores/monitor.svelte';

export interface MonitorViewProps {
	sourceName?: string;
	verified: boolean;
	monitoring: boolean;
	transition?: MonitorTransition;
	recordingState?: RecordingStatus | null;
	cvLanguage?: 'en' | 'jp' | null;
	replaySaves?: ReplaySaveStatus[];
	match?: LevelMatch | null;
	fps?: MonitorFps | null;
	showMonitorFps?: boolean;
	recentRuns?: RunClip[];
	recentRunsBusyId?: string | null;
	recentRunsError?: string | null;
	onKeepRun?: (runId: string) => void;
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
	fpsWarning: boolean;
	fpsLagging: boolean;
}

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
		? fps.capturedFps > 0
			? `${fps.processedFps.toFixed(1)} / ${fps.capturedFps.toFixed(1)} FPS`
			: `${fps.processedFps.toFixed(1)} FPS`
		: null;

	return {
		waitingForObs,
		statusLabel:
			transition === 'starting' ? 'Starting monitor' : transition === 'stopping' ? 'Stopping monitor' : 'Monitoring',
		title,
		detail,
		showDetail: waitingForObs || detail.trim().toLowerCase() !== 'unknown',
		phase: monitorPresentationPhase(recordingState, waitingForObs, verified),
		animationKey: [
			verified ? 'verified' : 'unverified',
			transition ? `transition-${transition}` : (recordingState ?? 'waiting')
		].join(':'),
		fpsText,
		fpsWarning: fps?.health === 'warning',
		fpsLagging: fps?.health === 'lagging'
	};
};

export const formatMonitorTime = (secs: number): string => {
	const m = Math.floor(secs / 60);
	const s = secs % 60;
	return `${m}:${s.toString().padStart(2, '0')}`;
};
