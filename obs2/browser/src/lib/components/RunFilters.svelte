<script lang="ts">
	import Select, { type SelectOption } from '$lib/components/Select.svelte';
	import { DIFFICULTY_OPTIONS, LANGUAGE_OPTIONS, STATUS_OPTIONS, type RunFilters } from '$lib/utils/runsView';

	let {
		collapsed = $bindable(),
		filters,
		activeFilterLabels,
		hasActiveFilters,
		levelOptions,
		clearFilters
	}: {
		collapsed: boolean;
		filters: RunFilters;
		activeFilterLabels: string[];
		hasActiveFilters: boolean;
		levelOptions: SelectOption[];
		clearFilters: () => void;
	} = $props();
</script>

<form
	class="obs-panel sticky top-0 z-20 mb-4 grid gap-2 rounded px-3 py-3"
	onsubmit={(event) => event.preventDefault()}
>
	<div class="flex min-w-0 items-center gap-2">
		<button
			type="button"
			class="obs-text-button flex min-w-0 flex-1 items-center justify-between gap-2 px-2 py-1.5 font-mono text-xs"
			aria-expanded={!collapsed}
			aria-controls="runs-filter-controls"
			onclick={() => (collapsed = !collapsed)}
		>
			<span aria-hidden="true">{collapsed ? 'show' : 'hide'}</span>
			<span class="min-w-0 truncate">filters{activeFilterLabels.length ? ` (${activeFilterLabels.length})` : ''}</span>
		</button>

		<button
			type="button"
			class="obs-text-button shrink-0 px-2 py-1.5 font-mono text-xs"
			disabled={!hasActiveFilters}
			onclick={clearFilters}
		>
			clear
		</button>
	</div>

	<p class="obs-dim min-w-0 truncate font-mono text-xs" title={activeFilterLabels.join(' | ')}>
		{activeFilterLabels.length ? activeFilterLabels.join(' | ') : 'all runs'}
	</p>

	{#if !collapsed}
		<div id="runs-filter-controls" class="grid gap-2">
			<label class="sr-only" for="runs-search">Search runs</label>
			<input
				id="runs-search"
				class="obs-input px-3 py-2 font-mono text-sm"
				type="search"
				placeholder="search runs"
				bind:value={filters.search}
			/>
			<div class="grid grid-cols-2 gap-2">
				<label class="sr-only" for="runs-level">Level</label>
				<Select
					id="runs-level"
					class="w-full text-xs"
					bind:value={filters.level}
					options={[{ value: '', label: 'all levels' }, ...levelOptions]}
				/>

				<label class="sr-only" for="runs-difficulty">Difficulty</label>
				<Select
					id="runs-difficulty"
					class="w-full text-xs"
					bind:value={filters.difficulty}
					options={[{ value: '', label: 'all difficulties' }, ...DIFFICULTY_OPTIONS]}
				/>

				<label class="sr-only" for="runs-status">Status</label>
				<Select
					id="runs-status"
					class="w-full text-xs"
					bind:value={filters.status}
					options={[{ value: '', label: 'all statuses' }, ...STATUS_OPTIONS]}
				/>

				<label class="sr-only" for="runs-language">Language</label>
				<Select
					id="runs-language"
					class="w-full text-xs"
					bind:value={filters.language}
					options={[{ value: '', label: 'all languages' }, ...LANGUAGE_OPTIONS]}
				/>
			</div>
			<div class="grid grid-cols-2 gap-2">
				<label class="sr-only" for="runs-min-time">Minimum time</label>
				<input
					id="runs-min-time"
					class="obs-input px-2 py-2 font-mono text-xs"
					inputmode="numeric"
					placeholder="min time"
					bind:value={filters.minTime}
				/>

				<label class="sr-only" for="runs-max-time">Maximum time</label>
				<input
					id="runs-max-time"
					class="obs-input px-2 py-2 font-mono text-xs"
					inputmode="numeric"
					placeholder="max time"
					bind:value={filters.maxTime}
				/>
			</div>
		</div>
	{/if}
</form>
