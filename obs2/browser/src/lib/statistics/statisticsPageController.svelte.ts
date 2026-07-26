import type {
	Backend,
	DifficultyNumber,
	MonitoringSessionDetail,
	MonitoringSessionSummary,
	RunStatus,
	StatisticsBucket,
	StatisticsResponse
} from '$lib/api';
import {
	readStatisticsPreferences,
	writeStatisticsPreferences,
	type StatisticsImprovementSeries,
	type StatisticsLevelOrder,
	type StatisticsOutcomeMeasure,
	type StatisticsTab
} from '$lib/utils/statisticsPreferences';
import { defaultDateRange, resolveDateRange, type DateRangeSelection } from '$lib/utils/statisticsRange';
import { ALL_STATUSES } from '$lib/utils/statisticsView';
import { untrack } from 'svelte';
import { statisticsRouteHref, statisticsRouteState, type StatisticsRouteState } from './statisticsQuery';

type StatisticsBackend = Pick<Backend, 'getStatistics' | 'getStatisticsSessions' | 'getStatisticsSession'>;

export interface StatisticsPageNavigation {
	currentUrl: () => URL;
	goto: (href: string, options: { replaceState: boolean; noScroll: boolean; keepFocus: boolean }) => void;
}

const errorMessage = (error: unknown): string => (error instanceof Error ? error.message : String(error));

export class StatisticsPageController {
	range = $state<DateRangeSelection>(defaultDateRange());
	tab = $state<StatisticsTab>('overview');
	bucket = $state<StatisticsBucket>('week');
	levelNumber = $state(1);
	difficultyNumber = $state<DifficultyNumber>(0);
	data = $state<StatisticsResponse | null>(null);
	sessions = $state<MonitoringSessionSummary[]>([]);
	selectedSessionId = $state('');
	attemptsByLevelStatuses = $state<RunStatus[]>([...ALL_STATUSES]);
	attemptsOverTimeStatuses = $state<RunStatus[]>([...ALL_STATUSES]);
	improvementSeries = $state<StatisticsImprovementSeries[]>(['running-best', 'complete']);
	outcomeStatuses = $state<RunStatus[]>([...ALL_STATUSES]);
	sessionStatuses = $state<RunStatus[]>([...ALL_STATUSES]);
	outcomeMeasure = $state<StatisticsOutcomeMeasure>('share');
	levelOrder = $state<StatisticsLevelOrder>('attempts');
	sessionDetail = $state<MonitoringSessionDetail | null>(null);
	loading = $state(false);
	sessionsLoading = $state(false);
	sessionDetailLoading = $state(false);
	error = $state<string | null>(null);

	private mounted = $state(false);
	private initialCohortResolved = $state(false);
	private storage: Storage | null = null;
	private dataAbort: AbortController | null = null;
	private sessionsAbort: AbortController | null = null;
	private sessionDetailAbort: AbortController | null = null;

	constructor(
		private readonly api: StatisticsBackend,
		private readonly navigation: StatisticsPageNavigation,
		initialUrl: URL
	) {
		const initial = statisticsRouteState(initialUrl, defaultDateRange());
		this.applyRouteState(initial);
		this.initialCohortResolved = initialUrl.searchParams.has('level') && initialUrl.searchParams.has('difficulty');
		this.setupEffects();
	}

	get resolvedRange() {
		return resolveDateRange(this.range);
	}

	get sessionLoading(): boolean {
		return this.sessionsLoading || this.sessionDetailLoading;
	}

	initialize(storage: Storage): void {
		this.storage = storage;
		const stored = readStatisticsPreferences(storage);
		const params = this.navigation.currentUrl().searchParams;
		if (stored) {
			if (!params.has('tab') && stored.tab) this.tab = stored.tab;
			if (!params.has('range') && stored.range) this.range = stored.range;
			if (!params.has('bucket') && stored.bucket) this.bucket = stored.bucket;
			if (!params.has('level') && stored.levelNumber) this.levelNumber = stored.levelNumber;
			if (!params.has('difficulty') && stored.difficultyNumber != null) {
				this.difficultyNumber = stored.difficultyNumber;
			}
			if (stored.levelNumber != null && stored.difficultyNumber != null) this.initialCohortResolved = true;
			if (stored.attemptsByLevelStatuses) this.attemptsByLevelStatuses = stored.attemptsByLevelStatuses;
			if (stored.attemptsOverTimeStatuses) this.attemptsOverTimeStatuses = stored.attemptsOverTimeStatuses;
			if (stored.improvementSeries) this.improvementSeries = stored.improvementSeries;
			if (stored.outcomeStatuses) this.outcomeStatuses = stored.outcomeStatuses;
			if (stored.sessionStatuses) this.sessionStatuses = stored.sessionStatuses;
			if (stored.outcomeMeasure) this.outcomeMeasure = stored.outcomeMeasure;
			if (stored.levelOrder) this.levelOrder = stored.levelOrder;
			if (stored.selectedSessionId) this.selectedSessionId = stored.selectedSessionId;
		}
		this.mounted = true;
	}

	destroy(): void {
		this.dataAbort?.abort();
		this.sessionsAbort?.abort();
		this.sessionDetailAbort?.abort();
	}

	private setupEffects(): void {
		$effect(() => {
			const mounted = this.mounted;
			const state = this.routeState();
			if (mounted) untrack(() => this.syncUrl(state));
		});

		$effect(() => {
			const mounted = this.mounted;
			const request = {
				range: { ...this.range },
				bucket: this.bucket,
				levelNumber: this.levelNumber,
				difficultyNumber: this.difficultyNumber,
				initialCohortResolved: this.initialCohortResolved
			};
			if (mounted) void untrack(() => this.loadStatistics(request));
		});

		$effect(() => {
			const mounted = this.mounted;
			const tab = this.tab;
			const range = { ...this.range };
			if (!mounted) return;
			if (tab === 'sessions') {
				void untrack(() => this.loadSessions(range));
			} else {
				this.sessionsAbort?.abort();
				this.sessionDetailAbort?.abort();
			}
		});

		$effect(() => {
			const mounted = this.mounted;
			const preferences = {
				version: 1 as const,
				tab: this.tab,
				range: { ...this.range },
				bucket: this.bucket,
				levelNumber: this.levelNumber,
				difficultyNumber: this.difficultyNumber,
				attemptsByLevelStatuses: [...this.attemptsByLevelStatuses],
				attemptsOverTimeStatuses: [...this.attemptsOverTimeStatuses],
				improvementSeries: [...this.improvementSeries],
				outcomeStatuses: [...this.outcomeStatuses],
				sessionStatuses: [...this.sessionStatuses],
				outcomeMeasure: this.outcomeMeasure,
				levelOrder: this.levelOrder,
				selectedSessionId: this.selectedSessionId
			};
			if (mounted && this.storage) untrack(() => writeStatisticsPreferences(this.storage!, preferences));
		});

		$effect(() => {
			const mounted = this.mounted;
			const tab = this.tab;
			const sessionId = this.selectedSessionId;
			if (mounted && tab === 'sessions') void untrack(() => this.loadSessionDetail(sessionId));
		});
	}

	private routeState(): StatisticsRouteState {
		return {
			tab: this.tab,
			range: { ...this.range },
			bucket: this.bucket,
			levelNumber: this.levelNumber,
			difficultyNumber: this.difficultyNumber
		};
	}

	private applyRouteState(state: StatisticsRouteState): void {
		this.tab = state.tab;
		this.range = state.range;
		this.bucket = state.bucket;
		this.levelNumber = state.levelNumber;
		this.difficultyNumber = state.difficultyNumber;
	}

	private syncUrl(state: StatisticsRouteState): void {
		const current = this.navigation.currentUrl();
		const target = statisticsRouteHref(current, state);
		if (target !== `${current.pathname}${current.search}`) {
			this.navigation.goto(target, { replaceState: true, noScroll: true, keepFocus: true });
		}
	}

	private async loadStatistics(request: {
		range: DateRangeSelection;
		bucket: StatisticsBucket;
		levelNumber: number;
		difficultyNumber: DifficultyNumber;
		initialCohortResolved: boolean;
	}): Promise<void> {
		this.dataAbort?.abort();
		const resolved = resolveDateRange(request.range);
		if (resolved.error) {
			this.loading = false;
			return;
		}
		const abort = new AbortController();
		this.dataAbort = abort;
		this.loading = true;
		this.error = null;
		try {
			const loaded = await this.api.getStatistics(
				{
					from: resolved.from,
					to: resolved.to,
					bucket: request.bucket,
					levelNumber: request.initialCohortResolved ? request.levelNumber : undefined,
					difficultyNumber: request.initialCohortResolved ? request.difficultyNumber : undefined
				},
				{ signal: abort.signal }
			);
			if (abort.signal.aborted) return;
			this.data = loaded;
			if (!request.initialCohortResolved) {
				if (loaded.selectedCohort) {
					this.levelNumber = loaded.selectedCohort.levelNumber;
					this.difficultyNumber = loaded.selectedCohort.difficultyNumber;
				}
				this.initialCohortResolved = true;
			}
		} catch (error) {
			if (!abort.signal.aborted) this.error = errorMessage(error);
		} finally {
			if (this.dataAbort === abort) {
				this.dataAbort = null;
				this.loading = false;
			}
		}
	}

	private async loadSessions(range: DateRangeSelection): Promise<void> {
		this.sessionsAbort?.abort();
		const resolved = resolveDateRange(range);
		if (resolved.error) {
			this.sessionsLoading = false;
			return;
		}
		const abort = new AbortController();
		this.sessionsAbort = abort;
		this.sessionsLoading = true;
		try {
			const sessions = await this.api.getStatisticsSessions(
				{ from: resolved.from, to: resolved.to },
				{ signal: abort.signal }
			);
			if (abort.signal.aborted) return;
			this.sessions = sessions;
			if (!this.sessions.some((session) => session.sessionId === this.selectedSessionId)) {
				this.selectedSessionId = this.sessions[0]?.sessionId ?? '';
			}
			if (!this.selectedSessionId) this.sessionDetail = null;
		} catch (error) {
			if (!abort.signal.aborted) this.error = errorMessage(error);
		} finally {
			if (this.sessionsAbort === abort) {
				this.sessionsAbort = null;
				this.sessionsLoading = false;
			}
		}
	}

	private async loadSessionDetail(sessionId: string): Promise<void> {
		this.sessionDetailAbort?.abort();
		if (!sessionId) {
			this.sessionDetail = null;
			return;
		}
		const abort = new AbortController();
		this.sessionDetailAbort = abort;
		this.sessionDetailLoading = true;
		try {
			const detail = await this.api.getStatisticsSession(sessionId, { signal: abort.signal });
			if (!abort.signal.aborted) this.sessionDetail = detail;
		} catch (error) {
			if (!abort.signal.aborted) this.error = errorMessage(error);
		} finally {
			if (this.sessionDetailAbort === abort) {
				this.sessionDetailAbort = null;
				this.sessionDetailLoading = false;
			}
		}
	}
}
