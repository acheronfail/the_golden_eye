import { describe, expect, it } from 'vitest';
import type { RunClip } from './api';
import {
	EMPTY_RUN_FILTERS,
	clipTimeSeconds,
	hasActiveRunFilters,
	parseRunTimeSeconds,
	visibleRunClips,
	type RunFilters
} from './runsView';

const filters = (overrides: Partial<RunFilters> = {}): RunFilters => ({ ...EMPTY_RUN_FILTERS, ...overrides });

const clip = (overrides: {
	fileName: string;
	path?: string;
	timestamp: string;
	level: string;
	levelNumber?: number;
	difficulty?: string;
	status: string;
	romLanguage?: string;
	time?: string;
	timeSeconds?: number;
	modified?: string;
}): RunClip => ({
	path: overrides.path ?? `/runs/${overrides.fileName}`,
	fileName: overrides.fileName,
	directory: '/runs',
	sizeBytes: 1024,
	modified: overrides.modified ?? null,
	durationSecs: 70,
	metadata: {
		timestamp: overrides.timestamp,
		time: overrides.time,
		timeSeconds: overrides.timeSeconds,
		level: overrides.level,
		levelNumber: overrides.levelNumber,
		difficulty: overrides.difficulty,
		status: overrides.status,
		romLanguage: overrides.romLanguage ?? 'en',
		sourceName: 'GoldenEye',
		comment: 'The Golden Eye',
		pluginVersion: '1.0.0'
	}
});

const clips = [
	clip({
		fileName: 'facility-0058.mov',
		timestamp: '2026-07-10T10:00:00Z',
		level: 'Facility',
		levelNumber: 2,
		difficulty: '00 Agent',
		status: 'complete',
		romLanguage: 'en',
		time: '00:58',
		timeSeconds: 58
	}),
	clip({
		fileName: 'dam-failed.mov',
		timestamp: '2026-07-11T10:00:00Z',
		level: 'Dam',
		levelNumber: 1,
		difficulty: 'Agent',
		status: 'failed',
		romLanguage: 'jp',
		time: '01:12',
		timeSeconds: 72
	}),
	clip({
		fileName: 'archives-completed.mov',
		timestamp: '2026-07-09T10:00:00Z',
		level: 'Archives',
		levelNumber: 11,
		difficulty: 'Secret Agent',
		status: 'completed',
		romLanguage: 'en',
		time: '00:42',
		timeSeconds: 42
	})
];

describe('runs view behaviour', () => {
	it('sorts visible runs newest first by timestamp', () => {
		expect(visibleRunClips(clips, filters()).map((run) => run.fileName)).toEqual([
			'dam-failed.mov',
			'facility-0058.mov',
			'archives-completed.mov'
		]);
	});

	it('filters by search text across filename and metadata', () => {
		expect(visibleRunClips(clips, filters({ search: 'facility 00 agent' })).map((run) => run.fileName)).toEqual([
			'facility-0058.mov'
		]);
	});

	it('combines level, difficulty, language, and normalized status filters', () => {
		expect(
			visibleRunClips(
				clips,
				filters({
					level: 'Archives',
					difficulty: 'Secret Agent',
					language: 'en',
					status: 'complete'
				})
			).map((run) => run.fileName)
		).toEqual(['archives-completed.mov']);
	});

	it('filters by inclusive mm:ss run time bounds', () => {
		expect(visibleRunClips(clips, filters({ minTime: '00:50', maxTime: '01:00' })).map((run) => run.fileName)).toEqual([
			'facility-0058.mov'
		]);
	});

	it('falls back to parsing metadata time when timeSeconds is absent', () => {
		const withoutSeconds = clip({
			fileName: 'parsed.mov',
			timestamp: '2026-07-11T10:00:00Z',
			level: 'Runway',
			status: 'complete',
			time: '02:03'
		});

		expect(clipTimeSeconds(withoutSeconds)).toBe(123);
	});

	it('parses plain seconds and rejects invalid mm:ss values', () => {
		expect(parseRunTimeSeconds('75')).toBe(75);
		expect(parseRunTimeSeconds('01:75')).toBeNull();
	});

	it('detects active filters after trimming search text', () => {
		expect(hasActiveRunFilters(filters({ search: '   ' }))).toBe(false);
		expect(hasActiveRunFilters(filters({ status: 'failed' }))).toBe(true);
	});
});
