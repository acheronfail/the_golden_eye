import type { LevelMatch, RunClip } from '$lib/api';
import type { MonitorDesign, MonitorViewProps } from '$lib/features/monitor/monitorView';
import { completedRun, failedRun } from '../../fixtures';

export const monitorMatch = (screen: string, times: LevelMatch['times'] = null): LevelMatch => ({
	screen,
	mission: 2,
	part: 1,
	difficulty: 0,
	detected_lang: 'en',
	times,
	runtime_ms: 8.4
});

export type MonitorStoryArgs = MonitorViewProps & { design?: MonitorDesign };

// Shared across every monitor-state story; each state file overrides the
// state-specific fields and renders one story per design.
export const monitorBaseArgs: MonitorStoryArgs = {
	sourceName: 'N64 Capture',
	verified: true,
	monitoring: true,
	recordingState: null,
	match: monitorMatch('unknown'),
	onStop: () => {}
};

export const monitorDesignArgs = {
	missionGlass: { design: 'mission-glass' },
	signalBand: { design: 'signal-band' },
	debug: { design: 'debug', cvLanguage: 'en' }
} satisfies Record<string, Partial<MonitorStoryArgs>>;

export const monitorRecentRuns: RunClip[] = [
	{ ...failedRun, runId: 'recent-pending', retentionState: 'pending' },
	{ ...completedRun, runId: 'recent-ready', retentionState: 'pending' },
	{ ...completedRun, runId: 'recent-kept', retentionState: 'kept', retentionReason: 'manual' },
	{ ...completedRun, runId: 'recent-expired', path: '', retentionState: 'expired' },
	{ ...completedRun, runId: 'recent-pb', retentionState: 'kept', retentionReason: 'personalBest' }
];

const recentRunSeeds = [
	['Control', 'Agent', 37, 'kia'],
	['Facility', '00 Agent', 58, 'complete'],
	['Runway', 'Secret Agent', 72, 'complete'],
	['Dam', 'Agent', 84, 'abort'],
	['Archives', '00 Agent', 91, 'complete'],
	['Silo', 'Secret Agent', 103, 'complete'],
	['Train', 'Agent', 117, 'failed'],
	['Frigate', '00 Agent', 128, 'complete'],
	['Caverns', 'Secret Agent', 142, 'kia'],
	['Cradle', '00 Agent', 154, 'complete'],
	['Jungle', 'Agent', 169, 'abort'],
	['Aztec', '00 Agent', 183, 'complete']
] as const;

const retentionStateForIndex = (index: number): RunClip['retentionState'] => {
	switch (index % 4) {
		case 0:
			return 'pending';
		case 1:
		case 3:
			return 'kept';
		case 2:
			return 'expired';
		default:
			throw new Error('Modulo produced an unexpected retention-state index');
	}
};

const retentionReasonForIndex = (index: number, state: RunClip['retentionState']): string => {
	if (index % 4 === 3) return 'personalBest';
	if (state === 'kept') return 'manual';
	return 'recent';
};

export const longMonitorRecentRuns: RunClip[] = recentRunSeeds.map(
	([level, difficulty, timeSeconds, status], index) => {
		const retentionState = retentionStateForIndex(index);
		const retentionReason = retentionReasonForIndex(index, retentionState);
		const time = `${Math.floor(timeSeconds / 60)
			.toString()
			.padStart(2, '0')}:${(timeSeconds % 60).toString().padStart(2, '0')}`;
		const timestamp = new Date(Date.UTC(2026, 6, 22, 12, 0, 0) - index * 3_600_000).toISOString();

		return {
			...completedRun,
			runId: `monitor-recent-${index + 1}`,
			path: retentionState === 'expired' ? '' : `/runs/${level.toLowerCase().replaceAll(' ', '-')}-${index + 1}.mp4`,
			fileName: `${level} - ${difficulty} - ${time.replace(':', '-')}.mp4`,
			modified: timestamp,
			durationSecs: timeSeconds + 15,
			retentionState,
			retentionReason,
			metadata: {
				...completedRun.metadata,
				timestamp,
				time,
				timeSeconds,
				level,
				difficulty,
				status
			}
		};
	}
);
