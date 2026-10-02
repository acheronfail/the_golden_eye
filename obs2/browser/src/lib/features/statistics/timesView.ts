import type { RunClip } from '$lib/api';
import { clipTimeSeconds, DIFFICULTY_OPTIONS } from '$lib/features/runs/runsView';
import { LEVEL_NAMES } from './statisticsView';

export const TIME_DIFFICULTIES = DIFFICULTY_OPTIONS.map(({ value }) => value);

export function timesForLevel(runs: RunClip[], level: number, difficulty: string): RunClip[] {
	return runs
		.filter((run) => {
			const metadata = run.metadata;
			const seconds = clipTimeSeconds(run);
			return (
				(metadata.status === 'complete' || metadata.status === 'completed') &&
				(metadata.levelNumber ?? LEVEL_NAMES.indexOf(metadata.level as (typeof LEVEL_NAMES)[number]) + 1) === level &&
				metadata.difficulty === difficulty &&
				seconds !== null &&
				seconds >= 0
			);
		})
		.sort(
			(a, b) => Date.parse(b.metadata.timestamp) - Date.parse(a.metadata.timestamp) || a.runId.localeCompare(b.runId)
		);
}

export function bestTime(runs: RunClip[]): number | null {
	return runs.reduce<number | null>((best, run) => {
		const seconds = clipTimeSeconds(run);
		return seconds !== null && (best === null || seconds < best) ? seconds : best;
	}, null);
}

export function timesHistoryHref(level: number, difficulty: string): string {
	return `/statistics?${new URLSearchParams({ tab: 'times', timesLevel: String(level), timesDifficulty: difficulty })}`;
}
