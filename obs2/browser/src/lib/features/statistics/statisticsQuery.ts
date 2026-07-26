import type { DifficultyNumber, StatisticsBucket } from '$lib/api';
import type { DateRangePreset, DateRangeSelection } from '$lib/features/statistics/statisticsRange';
import type { StatisticsTab } from '$lib/features/statistics/statisticsPreferences';

export interface StatisticsRouteState {
	tab: StatisticsTab;
	range: DateRangeSelection;
	bucket: StatisticsBucket;
	levelNumber: number;
	difficultyNumber: DifficultyNumber;
}

export const parseStatisticsTab = (value: string | null): StatisticsTab =>
	['overview', 'improvement', 'outcomes', 'sessions'].includes(value ?? '') ? (value as StatisticsTab) : 'overview';

export const parseDateRangePreset = (value: string | null): DateRangePreset =>
	['today', '7d', '30d', '12m', 'all', 'custom'].includes(value ?? '') ? (value as DateRangePreset) : '30d';

export const parseStatisticsBucket = (value: string | null): StatisticsBucket =>
	['day', 'week', 'month', 'year'].includes(value ?? '') ? (value as StatisticsBucket) : 'week';

export const parseLevelNumber = (value: string | null): number => {
	const parsed = Number(value);
	return Number.isInteger(parsed) && parsed >= 1 && parsed <= 20 ? parsed : 1;
};

export const parseDifficultyNumber = (value: string | null): DifficultyNumber => {
	const parsed = Number(value);
	return Number.isInteger(parsed) && parsed >= 0 && parsed <= 3 ? (parsed as DifficultyNumber) : 0;
};

export function statisticsRouteState(url: URL, defaultRange: DateRangeSelection): StatisticsRouteState {
	return {
		tab: parseStatisticsTab(url.searchParams.get('tab')),
		range: {
			preset: parseDateRangePreset(url.searchParams.get('range')),
			customFrom: url.searchParams.get('fromDate') ?? defaultRange.customFrom,
			customTo: url.searchParams.get('toDate') ?? defaultRange.customTo
		},
		bucket: parseStatisticsBucket(url.searchParams.get('bucket')),
		levelNumber: parseLevelNumber(url.searchParams.get('level')),
		difficultyNumber: parseDifficultyNumber(url.searchParams.get('difficulty'))
	};
}

export function statisticsRouteHref(currentUrl: URL, state: StatisticsRouteState): string {
	const url = new URL(currentUrl);
	const params = url.searchParams;
	state.tab === 'overview' ? params.delete('tab') : params.set('tab', state.tab);
	state.range.preset === '30d' ? params.delete('range') : params.set('range', state.range.preset);
	state.bucket === 'week' ? params.delete('bucket') : params.set('bucket', state.bucket);
	state.levelNumber === 1 ? params.delete('level') : params.set('level', String(state.levelNumber));
	state.difficultyNumber === 0 ? params.delete('difficulty') : params.set('difficulty', String(state.difficultyNumber));
	if (state.range.preset === 'custom') {
		params.set('fromDate', state.range.customFrom);
		params.set('toDate', state.range.customTo);
	} else {
		params.delete('fromDate');
		params.delete('toDate');
	}
	return `${url.pathname}${url.search}`;
}
