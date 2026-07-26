import type { MonitoringSessionDetail, MonitoringSessionSummary, StatisticsResponse, StatusCounts } from '$lib/api';

const counts = (complete: number, failed: number, abort: number, kia: number): StatusCounts => ({
	total: complete + failed + abort + kia,
	complete,
	failed,
	abort,
	kia
});

export const statisticsFixture: StatisticsResponse = {
	range: {
		from: '2026-06-25T00:00:00+10:00',
		to: '2026-07-25T00:00:00+10:00',
		bucket: 'week',
		timeZone: 'AEST'
	},
	summary: {
		counts: counts(18, 39, 27, 6),
		totalSessionSeconds: 29_460,
		combinedBestTimes: {
			overallSeconds: 32400,
			recordedCells: 47,
			totalCells: 60,
			byDifficulty: [
				{ difficultyNumber: 0, totalSeconds: 9400, recordedLevels: 18, totalLevels: 20 },
				{ difficultyNumber: 1, totalSeconds: 10800, recordedLevels: 16, totalLevels: 20 },
				{ difficultyNumber: 2, totalSeconds: 12200, recordedLevels: 13, totalLevels: 20 }
			]
		}
	},
	byLevel: [
		{ levelNumber: 1, counts: counts(5, 10, 8, 1) },
		{ levelNumber: 2, counts: counts(8, 19, 11, 3) },
		{ levelNumber: 7, counts: counts(5, 10, 8, 2) }
	],
	overallBuckets: [
		{ start: '2026-06-29T00:00:00+10:00', end: '2026-07-06T00:00:00+10:00', counts: counts(3, 7, 9, 1) },
		{ start: '2026-07-06T00:00:00+10:00', end: '2026-07-13T00:00:00+10:00', counts: counts(4, 10, 7, 2) },
		{ start: '2026-07-13T00:00:00+10:00', end: '2026-07-20T00:00:00+10:00', counts: counts(5, 12, 6, 2) },
		{ start: '2026-07-20T00:00:00+10:00', end: '2026-07-27T00:00:00+10:00', counts: counts(6, 10, 5, 1) }
	],
	selectedCohort: {
		levelNumber: 1,
		difficultyNumber: 0,
		counts: counts(5, 10, 8, 1),
		buckets: [],
		runTimes: [
			{ runId: '1', completedAt: '2026-07-01T19:00:00+10:00', status: 'abort', timeSeconds: 83 },
			{ runId: '2', completedAt: '2026-07-04T19:00:00+10:00', status: 'failed', timeSeconds: 79 },
			{ runId: '3', completedAt: '2026-07-10T19:00:00+10:00', status: 'complete', timeSeconds: 76 },
			{ runId: '4', completedAt: '2026-07-19T19:00:00+10:00', status: 'complete', timeSeconds: 71 }
		]
	}
};

export const sessionSummaryFixture: MonitoringSessionSummary = {
	sessionId: 'session-1',
	startedAt: '2026-07-24T19:00:00+10:00',
	endedAt: '2026-07-24T21:10:00+10:00',
	sourceName: 'N64 Capture',
	initialCvLanguage: 'en',
	pluginVersion: '2.4.0',
	endReason: 'userStopped',
	counts: counts(3, 9, 12, 1),
	distinctLevels: 2
};

export const sessionDetailFixture: MonitoringSessionDetail = {
	...sessionSummaryFixture,
	attempts: [
		{
			runId: 's1',
			completedAt: '2026-07-24T19:04:00+10:00',
			elapsedSeconds: 240,
			levelNumber: 1,
			difficultyNumber: 0,
			status: 'abort',
			timeSeconds: 83
		},
		{
			runId: 's2',
			completedAt: '2026-07-24T19:12:00+10:00',
			elapsedSeconds: 720,
			levelNumber: 1,
			difficultyNumber: 0,
			status: 'complete',
			timeSeconds: 71
		}
	]
};
