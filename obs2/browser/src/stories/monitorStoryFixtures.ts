import type { LevelMatch, RunClip } from '$lib/api';
import { completedRun } from './fixtures';

export const monitorMatch = (screen: string, times: LevelMatch['times'] = null): LevelMatch => ({
	screen,
	mission: 2,
	part: 1,
	difficulty: 0,
	detected_lang: 'en',
	times,
	runtime_ms: 8.4
});

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

export const longMonitorRecentRuns: RunClip[] = recentRunSeeds.map(
	([level, difficulty, timeSeconds, status], index) => {
		const retentionState = index % 4 === 0 ? 'pending' : index % 4 === 2 ? 'expired' : 'kept';
		const retentionReason = index % 4 === 3 ? 'personalBest' : retentionState === 'kept' ? 'manual' : 'recent';
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
