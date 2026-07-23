<script lang="ts">
	import type { RunSort } from '$lib/api';
	import { RUN_SORT_OPTIONS } from '$lib/utils/runsView';

	let {
		sort,
		onChange
	}: {
		sort: RunSort;
		onChange: (sort: RunSort) => void;
	} = $props();

	let open = $state(false);
	let trigger = $state<HTMLButtonElement>();
	const label = $derived(RUN_SORT_OPTIONS.find((option) => option.value === sort)?.label ?? 'Newest first');

	function select(next: RunSort) {
		open = false;
		onChange(next);
		trigger?.focus();
	}

	function closeOnEscape(event: KeyboardEvent) {
		if (event.key !== 'Escape' || !open) return;
		open = false;
		trigger?.focus();
	}
</script>

<svelte:window onclick={() => open && (open = false)} onkeydown={closeOnEscape} />

<div class="relative">
	<button
		bind:this={trigger}
		type="button"
		class="obs-text-button gap-1.5 px-2 py-1 font-mono text-xs"
		class:obs-icon-button-open={open}
		aria-label={`Sort runs, current: ${label}`}
		aria-haspopup="menu"
		aria-expanded={open}
		onclick={(event) => {
			event.stopPropagation();
			open = !open;
		}}
	>
		<span aria-hidden="true">⇅</span>
		{label}
		<span aria-hidden="true">⌄</span>
	</button>

	{#if open}
		<div class="obs-menu-panel absolute right-0 z-30 mt-1 grid w-40 rounded p-1" role="menu" aria-label="Sort runs">
			{#each RUN_SORT_OPTIONS as option}
				<button
					type="button"
					role="menuitemradio"
					aria-checked={sort === option.value}
					class="obs-menu-link rounded px-3 py-2 text-left font-mono text-xs"
					class:obs-menu-link-active={sort === option.value}
					onclick={() => select(option.value)}
				>
					{option.label}
				</button>
			{/each}
		</div>
	{/if}
</div>
