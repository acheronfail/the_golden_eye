import type { DifficultyNumber, RunStatus, StatisticsBucket } from '$lib/api';
import { isLocalDateValue, type DateRangeSelection } from './statisticsRange';

export type StatisticsTab = 'overview' | 'improvement' | 'outcomes' | 'sessions';
export type StatisticsLevelOrder = 'attempts' | 'mission';
export type StatisticsOutcomeMeasure = 'share' | 'count';

export interface StatisticsPreferences {
	version: 1;
	tab: StatisticsTab;
	range: DateRangeSelection;
	bucket: StatisticsBucket;
	levelNumber: number;
	difficultyNumber: DifficultyNumber;
	improvementStatuses: RunStatus[];
	outcomeStatuses: RunStatus[];
	outcomeMeasure: StatisticsOutcomeMeasure;
	levelOrder: StatisticsLevelOrder;
	selectedSessionId: string;
}

type StoredPreferences = Partial<Omit<StatisticsPreferences, 'version'>> & { version: 1 };
type StorageReader = Pick<Storage, 'getItem'>;
type StorageWriter = Pick<Storage, 'setItem'>;

export const STATISTICS_PREFERENCES_STORAGE_KEY = 'the-golden-eye.statistics-preferences';

const tabs: StatisticsTab[] = ['overview', 'improvement', 'outcomes', 'sessions'];
const presets: DateRangeSelection['preset'][] = ['today', '7d', '30d', '12m', 'all', 'custom'];
const buckets: StatisticsBucket[] = ['day', 'week', 'month'];
const statuses: RunStatus[] = ['complete', 'failed', 'abort', 'kia'];
function isRecord(value: unknown): value is Record<string, unknown> {
	return typeof value === 'object' && value !== null && !Array.isArray(value);
}

function storedStatuses(value: unknown): RunStatus[] | undefined {
	if (!Array.isArray(value)) return undefined;
	const normalized = [
		...new Set(value.filter((status): status is RunStatus => statuses.includes(status as RunStatus)))
	];
	return normalized.length > 0 ? normalized : undefined;
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
		const improvementStatuses = storedStatuses(parsed.improvementStatuses);
		if (improvementStatuses) stored.improvementStatuses = improvementStatuses;
		const outcomeStatuses = storedStatuses(parsed.outcomeStatuses);
		if (outcomeStatuses) stored.outcomeStatuses = outcomeStatuses;
		if (parsed.outcomeMeasure === 'share' || parsed.outcomeMeasure === 'count') {
			stored.outcomeMeasure = parsed.outcomeMeasure;
		}
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
