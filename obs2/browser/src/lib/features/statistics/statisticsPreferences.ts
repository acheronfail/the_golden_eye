import type { DifficultyNumber, RunStatus, StatisticsBucket } from '$lib/api';
import { isLocalDateValue, type DateRangeSelection } from './statisticsRange';

export type StatisticsTab = 'overview' | 'improvement' | 'outcomes' | 'sessions';
export type StatisticsLevelOrder = 'attempts' | 'mission';
export type StatisticsOutcomeMeasure = 'share' | 'count';
export type StatisticsLevelMeasure = 'attempts' | 'time';
export type StatisticsLevelDifficulty = 0 | 1 | 2;
export type StatisticsImprovementSeries = RunStatus | 'running-best';

export interface StatisticsPreferences {
	version: 1;
	tab: StatisticsTab;
	range: DateRangeSelection;
	bucket: StatisticsBucket;
	levelNumber: number;
	difficultyNumber: DifficultyNumber;
	levelDifficulties: StatisticsLevelDifficulty[];
	attemptsOverTimeStatuses: RunStatus[];
	improvementSeries: StatisticsImprovementSeries[];
	outcomeStatuses: RunStatus[];
	sessionStatuses: RunStatus[];
	outcomeMeasure: StatisticsOutcomeMeasure;
	levelMeasure: StatisticsLevelMeasure;
	levelOrder: StatisticsLevelOrder;
	selectedSessionId: string;
}

type StoredPreferences = Partial<Omit<StatisticsPreferences, 'version'>> & { version: 1 };
type StorageReader = Pick<Storage, 'getItem'>;
type StorageWriter = Pick<Storage, 'setItem'>;

export const STATISTICS_PREFERENCES_STORAGE_KEY = 'the-golden-eye.statistics-preferences';

const tabs: StatisticsTab[] = ['overview', 'improvement', 'outcomes', 'sessions'];
const presets: DateRangeSelection['preset'][] = ['today', '7d', '30d', '12m', 'all', 'custom'];
const buckets: StatisticsBucket[] = ['day', 'week', 'month', 'year'];
const statuses: RunStatus[] = ['complete', 'failed', 'abort', 'kia'];
const improvementSeries: StatisticsImprovementSeries[] = [...statuses, 'running-best'];
function isRecord(value: unknown): value is Record<string, unknown> {
	return typeof value === 'object' && value !== null && !Array.isArray(value);
}

function storedStatuses(value: unknown): RunStatus[] | undefined {
	if (!Array.isArray(value)) return undefined;
	return [...new Set(value.filter((status): status is RunStatus => statuses.includes(status as RunStatus)))];
}

function storedImprovementSeries(value: unknown): StatisticsImprovementSeries[] | undefined {
	if (!Array.isArray(value)) return undefined;
	return [
		...new Set(
			value.filter((series): series is StatisticsImprovementSeries =>
				improvementSeries.includes(series as StatisticsImprovementSeries)
			)
		)
	];
}

function storedLevelDifficulties(value: unknown): StatisticsLevelDifficulty[] | undefined {
	if (!Array.isArray(value)) return undefined;
	return [
		...new Set(
			value.filter(
				(difficulty): difficulty is StatisticsLevelDifficulty =>
					typeof difficulty === 'number' && [0, 1, 2].includes(difficulty)
			)
		)
	];
}

export function readStatisticsPreferences(storage: StorageReader): StoredPreferences | null {
	try {
		const parsed: unknown = JSON.parse(storage.getItem(STATISTICS_PREFERENCES_STORAGE_KEY) ?? 'null');
		if (!isRecord(parsed) || parsed.version !== 1) return null;

		const stored: StoredPreferences = { version: 1 };
		if (tabs.includes(parsed.tab as StatisticsTab)) stored.tab = parsed.tab as StatisticsTab;
		if (buckets.includes(parsed.bucket as StatisticsBucket)) stored.bucket = parsed.bucket as StatisticsBucket;
		if (parsed.levelNumber != null) {
			const level = Number(parsed.levelNumber);
			if (Number.isInteger(level) && level >= 1 && level <= 20) stored.levelNumber = level;
		}
		if (parsed.difficultyNumber != null) {
			const difficulty = Number(parsed.difficultyNumber);
			if (Number.isInteger(difficulty) && difficulty >= 0 && difficulty <= 3) {
				stored.difficultyNumber = difficulty as DifficultyNumber;
			}
		}
		if (isRecord(parsed.range) && presets.includes(parsed.range.preset as DateRangeSelection['preset'])) {
			stored.range = {
				preset: parsed.range.preset as DateRangeSelection['preset'],
				customFrom:
					typeof parsed.range.customFrom === 'string' && isLocalDateValue(parsed.range.customFrom)
						? parsed.range.customFrom
						: '',
				customTo:
					typeof parsed.range.customTo === 'string' && isLocalDateValue(parsed.range.customTo)
						? parsed.range.customTo
						: ''
			};
		}
		const selectedImprovementSeries = storedImprovementSeries(parsed.improvementSeries);
		if (selectedImprovementSeries) {
			stored.improvementSeries = selectedImprovementSeries;
		} else {
			const legacyStatuses = storedStatuses(parsed.improvementStatuses);
			if (legacyStatuses) stored.improvementSeries = [...legacyStatuses, 'running-best'];
		}
		const levelDifficulties = storedLevelDifficulties(parsed.levelDifficulties);
		if (levelDifficulties) stored.levelDifficulties = levelDifficulties;
		const attemptsOverTimeStatuses = storedStatuses(parsed.attemptsOverTimeStatuses);
		if (attemptsOverTimeStatuses) stored.attemptsOverTimeStatuses = attemptsOverTimeStatuses;
		const outcomeStatuses = storedStatuses(parsed.outcomeStatuses);
		if (outcomeStatuses) stored.outcomeStatuses = outcomeStatuses;
		const sessionStatuses = storedStatuses(parsed.sessionStatuses);
		if (sessionStatuses) stored.sessionStatuses = sessionStatuses;
		if (parsed.outcomeMeasure === 'share' || parsed.outcomeMeasure === 'count') {
			stored.outcomeMeasure = parsed.outcomeMeasure;
		}
		if (parsed.levelMeasure === 'attempts' || parsed.levelMeasure === 'time') stored.levelMeasure = parsed.levelMeasure;
		if (parsed.levelOrder === 'attempts' || parsed.levelOrder === 'mission') stored.levelOrder = parsed.levelOrder;
		if (typeof parsed.selectedSessionId === 'string') stored.selectedSessionId = parsed.selectedSessionId;
		return stored;
	} catch {
		return null;
	}
}

export function writeStatisticsPreferences(storage: StorageWriter, preferences: StatisticsPreferences): void {
	try {
		storage.setItem(STATISTICS_PREFERENCES_STORAGE_KEY, JSON.stringify(preferences));
	} catch {
		// Statistics remain usable when browser storage is unavailable.
	}
}
