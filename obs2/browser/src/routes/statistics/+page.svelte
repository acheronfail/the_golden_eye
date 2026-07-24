<script lang="ts">
	import { goto } from '$app/navigation';
	import { page } from '$app/state';
	import {
		backend,
		type DifficultyNumber,
		type MonitoringSessionDetail,
		type MonitoringSessionSummary,
		type RunStatus,
		type StatisticsBucket,
		type StatisticsResponse
	} from '$lib/api';
	import DateRangeSelect from '$lib/components/DateRangeSelect.svelte';
	import SectionTitle from '$lib/components/SectionTitle.svelte';
	import StatisticsDashboard from '$lib/components/StatisticsDashboard.svelte';
	import {
		defaultDateRange,
		resolveDateRange,
		type DateRangePreset,
		type DateRangeSelection
	} from '$lib/utils/statisticsRange';
	import {
		readStatisticsPreferences,
		writeStatisticsPreferences,
		type StatisticsImprovementSeries,
		type StatisticsLevelOrder,
		type StatisticsOutcomeMeasure,
		type StatisticsTab
	} from '$lib/utils/statisticsPreferences';
	import { ALL_STATUSES } from '$lib/utils/statisticsView';
	import { onDestroy, onMount } from 'svelte';

	const validTab = (value: string | null): StatisticsTab =>
		['overview', 'improvement', 'outcomes', 'sessions'].includes(value ?? '') ? (value as StatisticsTab) : 'overview';
	const validPreset = (value: string | null): DateRangePreset =>
		['today', '7d', '30d', '12m', 'all', 'custom'].includes(value ?? '') ? (value as DateRangePreset) : '30d';
	const validBucket = (value: string | null): StatisticsBucket =>
		['day', 'week', 'month'].includes(value ?? '') ? (value as StatisticsBucket) : 'week';
	const validLevel = (value: string | null): number => {
		const parsed = Number(value);
		return Number.isInteger(parsed) && parsed >= 1 && parsed <= 20 ? parsed : 1;
	};
	const validDifficulty = (value: string | null): DifficultyNumber => {
		const parsed = Number(value);
		return Number.isInteger(parsed) && parsed >= 0 && parsed <= 3 ? (parsed as DifficultyNumber) : 0;
	};

	const defaults = defaultDateRange();
	let range = $state<DateRangeSelection>({
		preset: validPreset(page.url.searchParams.get('range')),
		customFrom: page.url.searchParams.get('fromDate') ?? defaults.customFrom,
		customTo: page.url.searchParams.get('toDate') ?? defaults.customTo
	});
	let tab = $state<StatisticsTab>(validTab(page.url.searchParams.get('tab')));
	let bucket = $state<StatisticsBucket>(validBucket(page.url.searchParams.get('bucket')));
	let levelNumber = $state(validLevel(page.url.searchParams.get('level')));
	let difficultyNumber = $state<DifficultyNumber>(validDifficulty(page.url.searchParams.get('difficulty')));
	let initialCohortResolved = $state(page.url.searchParams.has('level') && page.url.searchParams.has('difficulty'));
	let data = $state<StatisticsResponse | null>(null);
	let sessions = $state<MonitoringSessionSummary[]>([]);
	let selectedSessionId = $state('');
	let attemptsByLevelStatuses = $state<RunStatus[]>([...ALL_STATUSES]);
	let attemptsOverTimeStatuses = $state<RunStatus[]>([...ALL_STATUSES]);
	let improvementSeries = $state<StatisticsImprovementSeries[]>(['running-best', 'complete']);
	let outcomeStatuses = $state<RunStatus[]>([...ALL_STATUSES]);
	let sessionStatuses = $state<RunStatus[]>([...ALL_STATUSES]);
	let outcomeMeasure = $state<StatisticsOutcomeMeasure>('share');
	let levelOrder = $state<StatisticsLevelOrder>('attempts');
	let sessionDetail = $state<MonitoringSessionDetail | null>(null);
	let loading = $state(false);
	let sessionsLoading = $state(false);
	let sessionDetailLoading = $state(false);
	let error = $state<string | null>(null);
	let mounted = $state(false);
	let dataAbort: AbortController | null = null;
	let sessionsAbort: AbortController | null = null;
	let sessionDetailAbort: AbortController | null = null;
	const resolvedRange = $derived(resolveDateRange(range));
	const sessionLoading = $derived(sessionsLoading || sessionDetailLoading);

	function syncUrl() {
		const url = new URL(page.url);
		const params = url.searchParams;
		tab === 'overview' ? params.delete('tab') : params.set('tab', tab);
		range.preset === '30d' ? params.delete('range') : params.set('range', range.preset);
		bucket === 'week' ? params.delete('bucket') : params.set('bucket', bucket);
		levelNumber === 1 ? params.delete('level') : params.set('level', String(levelNumber));
		difficultyNumber === 0 ? params.delete('difficulty') : params.set('difficulty', String(difficultyNumber));
		if (range.preset === 'custom') {
			params.set('fromDate', range.customFrom);
			params.set('toDate', range.customTo);
		} else {
			params.delete('fromDate');
			params.delete('toDate');
		}
		const target = `${url.pathname}${url.search}`;
		if (target !== `${page.url.pathname}${page.url.search}`) {
			void goto(target, { replaceState: true, noScroll: true, keepFocus: true });
		}
	}

	async function loadStatistics() {
		dataAbort?.abort();
		if (resolvedRange.error) {
			loading = false;
			return;
		}
		const abort = new AbortController();
		dataAbort = abort;
		loading = true;
		error = null;
		try {
			const loaded = await backend.getStatistics(
				{
					from: resolvedRange.from,
					to: resolvedRange.to,
					bucket,
					levelNumber: initialCohortResolved ? levelNumber : undefined,
					difficultyNumber: initialCohortResolved ? difficultyNumber : undefined
				},
				{ signal: abort.signal }
			);
			data = loaded;
			if (!initialCohortResolved) {
				if (loaded.selectedCohort) {
					levelNumber = loaded.selectedCohort.levelNumber;
					difficultyNumber = loaded.selectedCohort.difficultyNumber;
				}
				initialCohortResolved = true;
			}
		} catch (err) {
			if (!abort.signal.aborted) error = err instanceof Error ? err.message : String(err);
		} finally {
			if (dataAbort === abort) {
				dataAbort = null;
				loading = false;
			}
		}
	}

	async function loadSessions() {
		sessionsAbort?.abort();
		if (resolvedRange.error) {
			sessionsLoading = false;
			return;
		}
		const abort = new AbortController();
		sessionsAbort = abort;
		sessionsLoading = true;
		try {
			sessions = await backend.getStatisticsSessions(
				{ from: resolvedRange.from, to: resolvedRange.to },
				{ signal: abort.signal }
			);
			if (!sessions.some((session) => session.sessionId === selectedSessionId)) {
				selectedSessionId = sessions[0]?.sessionId ?? '';
			}
			if (!selectedSessionId) sessionDetail = null;
		} catch (err) {
			if (!abort.signal.aborted) error = err instanceof Error ? err.message : String(err);
		} finally {
			if (sessionsAbort === abort) {
				sessionsAbort = null;
				sessionsLoading = false;
			}
		}
	}

	async function loadSessionDetail(sessionId: string) {
		sessionDetailAbort?.abort();
		if (!sessionId) {
			sessionDetail = null;
			return;
		}
		const abort = new AbortController();
		sessionDetailAbort = abort;
		sessionDetailLoading = true;
		try {
			sessionDetail = await backend.getStatisticsSession(sessionId, { signal: abort.signal });
		} catch (err) {
			if (!abort.signal.aborted) error = err instanceof Error ? err.message : String(err);
		} finally {
			if (sessionDetailAbort === abort) {
				sessionDetailAbort = null;
				sessionDetailLoading = false;
			}
		}
	}

	onMount(() => {
		const stored = readStatisticsPreferences(localStorage);
		const params = page.url.searchParams;
		if (stored) {
			if (!params.has('tab') && stored.tab) tab = stored.tab;
			if (!params.has('range') && stored.range) range = stored.range;
			if (!params.has('bucket') && stored.bucket) bucket = stored.bucket;
			if (!params.has('level') && stored.levelNumber) levelNumber = stored.levelNumber;
			if (!params.has('difficulty') && stored.difficultyNumber != null) {
				difficultyNumber = stored.difficultyNumber;
			}
			if (stored.levelNumber != null && stored.difficultyNumber != null) initialCohortResolved = true;
			if (stored.attemptsByLevelStatuses) attemptsByLevelStatuses = stored.attemptsByLevelStatuses;
			if (stored.attemptsOverTimeStatuses) attemptsOverTimeStatuses = stored.attemptsOverTimeStatuses;
			if (stored.improvementSeries) improvementSeries = stored.improvementSeries;
			if (stored.outcomeStatuses) outcomeStatuses = stored.outcomeStatuses;
			if (stored.sessionStatuses) sessionStatuses = stored.sessionStatuses;
			if (stored.outcomeMeasure) outcomeMeasure = stored.outcomeMeasure;
			if (stored.levelOrder) levelOrder = stored.levelOrder;
			if (stored.selectedSessionId) selectedSessionId = stored.selectedSessionId;
		}
		mounted = true;
	});

	onDestroy(() => {
		dataAbort?.abort();
		sessionsAbort?.abort();
		sessionDetailAbort?.abort();
	});

	$effect(() => {
		range.preset;
		range.customFrom;
		range.customTo;
		tab;
		bucket;
		levelNumber;
		difficultyNumber;
		if (!mounted) return;
		syncUrl();
	});

	$effect(() => {
		range.preset;
		range.customFrom;
		range.customTo;
		bucket;
		levelNumber;
		difficultyNumber;
		initialCohortResolved;
		if (!mounted) return;
		void loadStatistics();
	});

	$effect(() => {
		range.preset;
		range.customFrom;
		range.customTo;
		if (!mounted) return;
		if (tab === 'sessions') {
			void loadSessions();
		} else {
			sessionsAbort?.abort();
			sessionDetailAbort?.abort();
		}
	});

	$effect(() => {
		if (!mounted) return;
		writeStatisticsPreferences(localStorage, {
			version: 1,
			tab,
			range,
			bucket,
			levelNumber,
			difficultyNumber,
			attemptsByLevelStatuses,
			attemptsOverTimeStatuses,
			improvementSeries,
			outcomeStatuses,
			sessionStatuses,
			outcomeMeasure,
			levelOrder,
			selectedSessionId
		});
	});

	$effect(() => {
		const sessionId = selectedSessionId;
		if (mounted && tab === 'sessions') void loadSessionDetail(sessionId);
	});
</script>

<svelte:head><title>Statistics</title></svelte:head>

<main class="mx-auto w-full max-w-3xl px-4 obs-page-top pb-4 sm:px-6 sm:pb-6">
	<header class="mb-4">
		<h1 class="text-2xl font-semibold obs-heading">Statistics</h1>
	</header>

	<StatisticsDashboard
		{data}
		{loading}
		{error}
		bind:levelNumber
		bind:difficultyNumber
		bind:tab
		{sessions}
		bind:selectedSessionId
		{sessionDetail}
		{sessionLoading}
		bind:attemptsByLevelStatuses
		bind:attemptsOverTimeStatuses
		bind:improvementSeries
		bind:outcomeStatuses
		bind:sessionStatuses
		bind:outcomeMeasure
		bind:levelOrder
	>
		{#snippet controls()}
			<section>
				<SectionTitle title="Filters" class="mb-3" />
				<DateRangeSelect bind:value={range} bind:bucket error={resolvedRange.error ?? null} />
			</section>
		{/snippet}
	</StatisticsDashboard>
</main>
