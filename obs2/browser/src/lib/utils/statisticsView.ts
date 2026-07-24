import {
	DIFFICULTY_LABELS,
	type BucketCounts,
	type DifficultyNumber,
	type MonitoringSessionDetail,
	type RunStatus,
	type StatisticsResponse
} from '$lib/api';
import type { ChartData, ChartSeries } from '$lib/components/Chart';

export const LEVEL_NAMES = [
	'Dam',
	'Facility',
	'Runway',
	'Surface 1',
	'Bunker 1',
	'Silo',
	'Frigate',
	'Surface 2',
	'Bunker 2',
	'Statue',
	'Archives',
	'Streets',
	'Depot',
	'Train',
	'Jungle',
	'Control',
	'Caverns',
	'Cradle',
	'Aztec',
	'Egypt'
] as const;

export const STATUS_LABELS: Record<RunStatus, string> = {
	complete: 'Complete',
	failed: 'Failed',
	abort: 'Aborted',
	kia: 'Killed in Action'
};

export const ALL_STATUSES: RunStatus[] = ['complete', 'failed', 'abort', 'kia'];

function statusSeries(status: RunStatus, points: ChartSeries['points']): ChartSeries {
	const complete = status === 'complete';
	return {
		id: status,
		label: STATUS_LABELS[status],
		points,
		color: complete ? 'var(--obs-success)' : 'var(--obs-danger)',
		surfaceColor: complete ? 'var(--obs-success-surface)' : 'var(--obs-danger-surface)',
		pattern: status === 'abort' ? 'diagonal' : status === 'kia' ? 'dots' : 'plain',
		shape:
			status === 'complete' ? 'circle' : status === 'failed' ? 'square' : status === 'abort' ? 'diamond' : 'triangle'
	};
}

export function levelLabel(levelNumber: number | null): string {
	return levelNumber == null ? 'Unknown' : (LEVEL_NAMES[levelNumber - 1] ?? `Level ${levelNumber}`);
}

export function difficultyLabel(value: DifficultyNumber | null): string {
	return value == null ? 'Unknown' : DIFFICULTY_LABELS[value];
}

export function formatDuration(seconds: number): string {
	const rounded = Math.max(0, Math.round(seconds));
	const hours = Math.floor(rounded / 3600);
	const minutes = Math.floor((rounded % 3600) / 60);
	const secs = rounded % 60;
	return hours > 0
		? `${hours}:${String(minutes).padStart(2, '0')}:${String(secs).padStart(2, '0')}`
		: `${minutes}:${String(secs).padStart(2, '0')}`;
}

export function attemptsByLevelData(response: StatisticsResponse, order: 'attempts' | 'mission'): ChartData {
	const levels = [...response.byLevel].sort((a, b) =>
		order === 'attempts'
			? b.counts.total - a.counts.total || (a.levelNumber ?? 99) - (b.levelNumber ?? 99)
			: (a.levelNumber ?? 99) - (b.levelNumber ?? 99)
	);
	return {
		kind: 'horizontalStackedBar',
		xType: 'category',
		series: ALL_STATUSES.map((status) =>
			statusSeries(
				status,
				levels.map((level) => ({ x: levelLabel(level.levelNumber), y: level.counts[status] }))
			)
		)
	};
}

export function overallAttemptsData(buckets: BucketCounts[]): ChartData {
	return {
		kind: 'stackedBar',
		xType: 'time',
		series: ALL_STATUSES.map((status) =>
			statusSeries(
				status,
				buckets.map((bucket) => ({
					x: new Date(bucket.start).getTime(),
					y: bucket.counts[status],
					label: new Date(bucket.start).toLocaleDateString()
				}))
			)
		)
	};
}

export function runTimeData(response: StatisticsResponse, statuses: RunStatus[]): ChartData {
	const cohort = response.selectedCohort;
	if (!cohort) return { kind: 'line', xType: 'time', series: [] };
	const series = statuses.map((status) =>
		statusSeries(
			status,
			cohort.runTimes
				.filter((run) => run.status === status)
				.map((run) => ({
					x: new Date(run.completedAt).getTime(),
					y: run.timeSeconds,
					label: new Date(run.completedAt).toLocaleString(),
					detail: `${levelLabel(cohort.levelNumber)} · ${difficultyLabel(cohort.difficultyNumber)}`
				}))
		)
	);
	let best: number | null = null;
	const bestPoints = cohort.runTimes
		.filter((run) => run.status === 'complete')
		.map((run) => {
			best = best == null ? run.timeSeconds : Math.min(best, run.timeSeconds);
			return { x: new Date(run.completedAt).getTime(), y: best, label: 'Completed running best' };
		});
	if (bestPoints.length > 0) {
		series.push({
			id: 'running-best',
			label: 'Completed running best',
			points: bestPoints,
			color: 'var(--obs-gold-hover)',
			surfaceColor: 'var(--obs-gold-surface)',
			pattern: 'plain',
			shape: 'circle'
		});
	}
	return { kind: 'line', xType: 'time', series };
}

export function outcomeData(buckets: BucketCounts[], statuses: RunStatus[], measure: 'share' | 'count'): ChartData {
	return {
		kind: 'groupedBar',
		xType: 'time',
		series: statuses.map((status) =>
			statusSeries(
				status,
				buckets.map((bucket) => {
					const selectedTotal = statuses.reduce((sum, selected) => sum + bucket.counts[selected], 0);
					const count = bucket.counts[status];
					return {
						x: new Date(bucket.start).getTime(),
						y: measure === 'share' && selectedTotal > 0 ? (count / selectedTotal) * 100 : count,
						label: new Date(bucket.start).toLocaleDateString(),
						detail: `${count} attempts · ${selectedTotal === 0 ? 0 : Math.round((count / selectedTotal) * 100)}%`
					};
				})
			)
		)
	};
}

export function sessionAttemptsData(session: MonitoringSessionDetail): ChartData {
	return {
		kind: 'line',
		xType: 'time',
		series: ALL_STATUSES.map((status) =>
			statusSeries(
				status,
				session.attempts
					.filter((attempt) => attempt.status === status && attempt.timeSeconds != null)
					.map((attempt) => ({
						x: attempt.elapsedSeconds * 1000,
						y: attempt.timeSeconds!,
						label: `${formatDuration(attempt.elapsedSeconds)} into session`,
						detail: `${levelLabel(attempt.levelNumber)} · ${difficultyLabel(attempt.difficultyNumber)}`
					}))
			)
		)
	};
}

export function mostPlayedLevel(response: StatisticsResponse): string {
	const level = [...response.byLevel].sort((a, b) => b.counts.total - a.counts.total)[0];
	return level ? levelLabel(level.levelNumber) : '—';
}
