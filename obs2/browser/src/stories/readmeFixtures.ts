import { statisticsFixture } from './features/statistics/statisticsFixtures';
import type { RunClip, StatisticsResponse } from '$lib/api';
import { completedRun } from './fixtures';

const samples = [
	[58, 'complete', 'kept', 'personalBest'],
	[62, 'complete', 'pending', 'recent'],
	[43, 'failed', 'pending', 'recent'],
	[61, 'complete', 'kept', 'personalBest'],
	[65, 'complete', 'kept', 'manual'],
	[66, 'complete', 'expired', 'recent'],
	[68, 'complete', 'kept', 'manual']
] as const;

export const readmeRuns: RunClip[] = samples.map(([seconds, status, retentionState, retentionReason], index) => {
	const timestamp = new Date(Date.UTC(2026, 6, 21, 12, 43 - index * 5)).toISOString();
	const time = `${Math.floor(seconds / 60)
		.toString()
		.padStart(2, '0')}:${(seconds % 60).toString().padStart(2, '0')}`;
	const fileName = `Facility - 00 Agent - ${time.replace(':', '-')}.mp4`;
	return {
		...completedRun,
		runId: `readme-run-${index}`,
		path: retentionState === 'expired' ? '' : `/runs/completed/${fileName}`,
		fileName: retentionState === 'expired' ? '' : fileName,
		modified: timestamp,
		durationSecs: seconds + 15,
		retentionState,
		retentionReason,
		metadata: {
			...completedRun.metadata,
			timestamp,
			time,
			timeSeconds: seconds,
			status,
			wasPersonalBest: retentionReason === 'personalBest'
		}
	};
});

export const readmeStatistics: StatisticsResponse = {
	...statisticsFixture,
	selectedCohort: {
		...statisticsFixture.selectedCohort!,
		runTimes: [91, 84, 88, 83, 85, 83, 82, 85, 81, 83, 75, 78, 75, 74, 77, 73].map((timeSeconds, index) => ({
			runId: `preview-${index}`,
			completedAt: new Date(
				Date.UTC(2026, 6, 1 + [0, 1, 2, 4, 5, 7, 10, 12, 13, 16, 20, 21, 22, 25, 27, 30][index], 9)
			).toISOString(),
			status: 'complete',
			timeSeconds
		}))
	}
};
