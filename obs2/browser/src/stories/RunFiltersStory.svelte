<script lang="ts">
	import RunFiltersForm from '$lib/components/RunFilters.svelte';
	import {
		EMPTY_RUN_FILTERS,
		LEVEL_OPTIONS,
		activeRunFilters,
		hasActiveRunFilters,
		type RunFilters
	} from '$lib/utils/runsView';

	let {
		initialFilters = {},
		initiallyCollapsed = false
	}: { initialFilters?: Partial<RunFilters>; initiallyCollapsed?: boolean } = $props();

	let collapsed = $state(false);
	let appliedCollapsed = $state<boolean | null>(null);
	let appliedFilters = $state<Partial<RunFilters> | null>(null);
	let filters = $state<RunFilters>({ ...EMPTY_RUN_FILTERS });
	const activeFilters = $derived(activeRunFilters(filters));
	const hasActiveFilters = $derived(hasActiveRunFilters(filters));
	const levelOptions = LEVEL_OPTIONS.map((level) => ({ value: level, label: level }));

	$effect(() => {
		if (initialFilters === appliedFilters) return;
		appliedFilters = initialFilters;
		Object.assign(filters, EMPTY_RUN_FILTERS, initialFilters);
	});

	$effect(() => {
		if (initiallyCollapsed === appliedCollapsed) return;
		appliedCollapsed = initiallyCollapsed;
		collapsed = initiallyCollapsed;
	});

	const clearFilters = () => {
		Object.assign(filters, EMPTY_RUN_FILTERS);
	};
	const clearFilter = (key: keyof RunFilters) => {
		filters[key] = '';
	};
</script>

<main class="mx-auto w-full max-w-3xl px-3 py-4 sm:px-4 sm:py-6">
	<RunFiltersForm
		bind:collapsed
		bind:filters
		{activeFilters}
		{hasActiveFilters}
		{levelOptions}
		{clearFilter}
		{clearFilters}
	/>
</main>
