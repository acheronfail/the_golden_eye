<script lang="ts">
	import Select, { type SelectOption } from '$lib/components/Select.svelte';
	import { onMount } from 'svelte';
	import {
		DIFFICULTY_OPTIONS,
		LANGUAGE_OPTIONS,
		STATUS_OPTIONS,
		type RunFilterKey,
		type RunFilters
	} from '$lib/utils/runsView';

	let {
		collapsed = $bindable(),
		filters,
		activeFilters,
		hasActiveFilters,
		levelOptions,
		clearFilter,
		clearFilters
	}: {
		collapsed: boolean;
		filters: RunFilters;
		activeFilters: { key: RunFilterKey; label: string }[];
		hasActiveFilters: boolean;
		levelOptions: SelectOption[];
		clearFilter: (key: RunFilterKey) => void;
		clearFilters: () => void;
	} = $props();

	let formElement: HTMLFormElement;
	let pinned = $state(false);

	onMount(() => {
		const scroller = formElement.closest<HTMLElement>('.obs-content-scroller');
		if (!scroller) return;

		const updatePinned = () => {
			pinned =
				scroller.scrollTop > 0 && formElement.getBoundingClientRect().top <= scroller.getBoundingClientRect().top + 1;
		};

		scroller.addEventListener('scroll', updatePinned, { passive: true });
		window.addEventListener('resize', updatePinned);
		updatePinned();

		return () => {
			scroller.removeEventListener('scroll', updatePinned);
			window.removeEventListener('resize', updatePinned);
		};
	});
</script>

<form
	bind:this={formElement}
	class="sticky top-0 z-20 mb-3 grid gap-2 border-b border-(--obs-border-muted) bg-(--obs-bg) py-2 transition-shadow"
	class:shadow-sm={pinned}
	onsubmit={(event) => event.preventDefault()}
>
	<div class="grid grid-cols-[minmax(0,1fr)_auto_auto] items-center gap-2">
		<label class="sr-only" for="runs-search">Search runs</label>
		<input
			id="runs-search"
			class="obs-input min-w-0 px-3 py-2 font-mono text-sm"
			type="search"
			placeholder="search level, file, time..."
			bind:value={filters.search}
		/>

		<button
			type="button"
			class="obs-text-button gap-1.5 px-2 py-2 font-mono text-xs"
			aria-expanded={!collapsed}
			aria-controls="runs-filter-controls"
			onclick={() => (collapsed = !collapsed)}
		>
			<span aria-hidden="true">⌁</span>
			filters{activeFilters.length ? ` (${activeFilters.length})` : ''}
		</button>

		<button
			type="button"
			class="obs-text-button px-2 py-2 font-mono text-xs"
			disabled={!hasActiveFilters}
			onclick={clearFilters}
		>
			clear
		</button>
	</div>

	{#if !collapsed}
		<div id="runs-filter-controls" class="obs-panel grid grid-cols-2 gap-2 rounded p-3 sm:grid-cols-3">
			<label class="grid gap-1 font-mono text-[10px] text-(--obs-text-dim)" for="runs-level">
				Level
				<Select
					id="runs-level"
					class="w-full text-xs text-(--obs-text)"
					bind:value={filters.level}
					options={[{ value: '', label: 'all levels' }, ...levelOptions]}
				/>
			</label>

			<label class="grid gap-1 font-mono text-[10px] text-(--obs-text-dim)" for="runs-difficulty">
				Difficulty
				<Select
					id="runs-difficulty"
					class="w-full text-xs text-(--obs-text)"
					bind:value={filters.difficulty}
					options={[{ value: '', label: 'all difficulties' }, ...DIFFICULTY_OPTIONS]}
				/>
			</label>

			<label class="grid gap-1 font-mono text-[10px] text-(--obs-text-dim)" for="runs-status">
				Status
				<Select
					id="runs-status"
					class="w-full text-xs text-(--obs-text)"
					bind:value={filters.status}
					options={[{ value: '', label: 'all statuses' }, ...STATUS_OPTIONS]}
				/>
			</label>

			<label class="grid gap-1 font-mono text-[10px] text-(--obs-text-dim)" for="runs-language">
				Language
				<Select
					id="runs-language"
					class="w-full text-xs text-(--obs-text)"
					bind:value={filters.language}
					options={[{ value: '', label: 'all languages' }, ...LANGUAGE_OPTIONS]}
				/>
			</label>

			<label class="grid gap-1 font-mono text-[10px] text-(--obs-text-dim)" for="runs-min-time">
				Minimum time
				<input
					id="runs-min-time"
					class="obs-input px-2 py-2 font-mono text-xs text-(--obs-text)"
					inputmode="numeric"
					placeholder="0:00"
					bind:value={filters.minTime}
				/>
			</label>

			<label class="grid gap-1 font-mono text-[10px] text-(--obs-text-dim)" for="runs-max-time">
				Maximum time
				<input
					id="runs-max-time"
					class="obs-input px-2 py-2 font-mono text-xs text-(--obs-text)"
					inputmode="numeric"
					placeholder="9:59"
					bind:value={filters.maxTime}
				/>
			</label>
		</div>
	{/if}

	{#if activeFilters.length}
		<div class="flex min-w-0 flex-wrap gap-1" aria-label="Active filters">
			{#each activeFilters as filter (filter.key)}
				<button
					type="button"
					class="obs-text-button max-w-full gap-1 px-2 py-1 font-mono text-[10px]"
					aria-label={`Remove ${filter.label} filter`}
					onclick={() => clearFilter(filter.key)}
				>
					<span class="truncate">{filter.label}</span><span aria-hidden="true">×</span>
				</button>
			{/each}
		</div>
	{/if}
</form>
