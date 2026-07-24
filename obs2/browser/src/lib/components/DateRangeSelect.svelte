<script lang="ts">
	import Select from './Select.svelte';
	import type { StatisticsBucket } from '$lib/api';
	import type { DateRangeSelection, DateRangePreset } from '$lib/utils/statisticsRange';

	let {
		value = $bindable(),
		bucket = $bindable(),
		error = null
	}: { value: DateRangeSelection; bucket: StatisticsBucket; error?: string | null } = $props();

	const options = [
		{ value: 'today', label: 'Today' },
		{ value: '7d', label: 'Last 7 days' },
		{ value: '30d', label: 'Last 30 days' },
		{ value: '12m', label: 'Last 12 months' },
		{ value: 'all', label: 'All time' },
		{ value: 'custom', label: 'Custom range' }
	];
	const bucketOptions = [
		{ value: 'day', label: 'Day' },
		{ value: 'week', label: 'Week' },
		{ value: 'month', label: 'Month' }
	];

	function updatePreset(preset: string) {
		value = { ...value, preset: preset as DateRangePreset };
	}
</script>

<div class="flex min-w-0 flex-wrap items-end gap-3">
	<label class="grid min-w-44 gap-1 text-xs font-semibold obs-muted" for="statistics-range">
		Date range
		<Select
			id="statistics-range"
			value={value.preset}
			{options}
			onChange={updatePreset}
			class="min-w-44 px-3 py-2 text-left text-sm"
		/>
	</label>
	{#if value.preset === 'custom'}
		<label class="grid min-w-36 flex-1 gap-1 text-xs font-semibold obs-muted">
			Start
			<input
				class="obs-input px-3 py-2 text-sm"
				type="date"
				value={value.customFrom}
				oninput={(event) => (value = { ...value, customFrom: event.currentTarget.value })}
			/>
		</label>
		<label class="grid min-w-36 flex-1 gap-1 text-xs font-semibold obs-muted">
			End
			<input
				class="obs-input px-3 py-2 text-sm"
				type="date"
				value={value.customTo}
				oninput={(event) => (value = { ...value, customTo: event.currentTarget.value })}
			/>
		</label>
	{/if}
	<label
		class="grid min-w-28 gap-1 text-xs font-semibold obs-muted"
		class:basis-full={value.preset === 'custom'}
		for="statistics-bucket"
	>
		Group by
		<Select
			id="statistics-bucket"
			value={bucket}
			options={bucketOptions}
			onChange={(next) => (bucket = next as StatisticsBucket)}
			class="px-3 py-2 text-left text-sm"
		/>
	</label>
</div>
{#if error}
	<p class="mt-2 text-sm text-(--obs-danger)" role="alert">{error}</p>
{/if}
