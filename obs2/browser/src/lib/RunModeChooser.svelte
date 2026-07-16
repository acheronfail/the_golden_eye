<script lang="ts">
	import type { MonitorRunMode } from '$lib/api';
	import { SINGLE_SEGMENT_CATEGORIES, difficultyId } from '$lib/singleSegment';

	let {
		open,
		sourceName,
		close,
		choose
	}: {
		open: boolean;
		sourceName: string | null;
		close: () => void;
		choose: (mode: MonitorRunMode, difficulty?: number) => void;
	} = $props();

	let difficultyCategory = $state<MonitorRunMode | null>(null);

	const reset = () => {
		difficultyCategory = null;
	};

	const closeAll = () => {
		reset();
		close();
	};

	const selectMode = (mode: MonitorRunMode) => {
		const category = SINGLE_SEGMENT_CATEGORIES.find((c) => c.id === mode);
		if (category?.selectDifficulty) {
			difficultyCategory = mode;
			return;
		}
		choose(mode);
		reset();
	};

	const selectDifficulty = (difficulty: string) => {
		if (!difficultyCategory) return;
		choose(difficultyCategory, difficultyId(difficulty));
		reset();
	};

	let difficultyOptions = $derived(
		SINGLE_SEGMENT_CATEGORIES.find((category) => category.id === difficultyCategory)?.difficulties ?? []
	);
</script>

{#if open}
	<div class="obs-overlay fixed inset-0 z-50 flex items-center justify-center p-4">
		<button type="button" aria-label="Close run mode chooser" class="absolute inset-0 cursor-default" onclick={closeAll}
		></button>
		<dialog open aria-label="Choose monitor mode" class="obs-dialog relative z-10 m-0 w-full max-w-md overflow-hidden rounded p-0">
			<header class="obs-dialog-header px-4 py-3">
				<h2 class="obs-heading text-lg font-semibold">
					{difficultyCategory ? 'Choose Any% difficulty' : 'Monitor source'}
				</h2>
				<p class="obs-dim mt-1 font-mono text-xs">
					{sourceName ?? 'Selected source'}
				</p>
			</header>
			<div class="grid gap-3 p-4">
				{#if difficultyCategory}
					{#each difficultyOptions as difficulty}
						<button type="button" class="obs-list-button grid gap-1 rounded px-3 py-3 text-left" onclick={() => selectDifficulty(difficulty)}>
							<span class="obs-list-title text-sm font-semibold">{difficulty}</span>
							<span class="obs-list-detail font-mono text-xs">Run Any% on {difficulty}</span>
						</button>
					{/each}
					<div class="flex justify-between">
						<button type="button" class="obs-text-button px-2 py-1 font-mono text-xs" onclick={reset}>back</button>
						<button type="button" class="obs-text-button px-2 py-1 font-mono text-xs" onclick={closeAll}>close</button>
					</div>
				{:else}
					<button type="button" class="obs-list-button grid gap-1 rounded px-3 py-3 text-left" onclick={() => selectMode('clips')}>
						<span class="obs-list-title text-sm font-semibold">Just monitor my run and save clips</span>
						<span class="obs-list-detail font-mono text-xs">Current behavior: detect stages and save replay-buffer clips.</span>
					</button>
					{#each SINGLE_SEGMENT_CATEGORIES as category}
						<button type="button" class="obs-list-button grid gap-1 rounded px-3 py-3 text-left" onclick={() => selectMode(category.id)}>
							<span class="obs-list-title text-sm font-semibold">Single segment run ({category.title})</span>
							<span class="obs-list-detail font-mono text-xs">{category.description}</span>
						</button>
					{/each}
					<div class="flex justify-end">
						<button type="button" class="obs-text-button px-2 py-1 font-mono text-xs" onclick={closeAll}>close</button>
					</div>
				{/if}
			</div>
		</dialog>
	</div>
{/if}
