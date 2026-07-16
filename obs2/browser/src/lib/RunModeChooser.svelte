<script lang="ts">
	import type { MonitorRunMode } from '$lib/api';
	import { SINGLE_SEGMENT_CATEGORIES, difficultyId } from '$lib/singleSegment';

	let {
		open,
		sourceName,
		close,
		choose,
		clipsAvailable = true
	}: {
		open: boolean;
		sourceName: string | null;
		close: () => void;
		choose: (mode: MonitorRunMode, difficulty?: number) => void;
		clipsAvailable?: boolean;
	} = $props();

	type Step = 'mode' | 'category' | 'difficulty';
	let step = $state<Step>('mode');
	let difficultyCategory = $state<MonitorRunMode | null>(null);

	const reset = () => {
		step = 'mode';
		difficultyCategory = null;
	};

	const closeAll = () => {
		reset();
		close();
	};

	const selectCategory = (mode: MonitorRunMode) => {
		const category = SINGLE_SEGMENT_CATEGORIES.find((c) => c.id === mode);
		if (category?.selectDifficulty) {
			difficultyCategory = mode;
			step = 'difficulty';
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

	const back = () => {
		if (step === 'difficulty') {
			difficultyCategory = null;
			step = 'category';
		} else {
			reset();
		}
	};

	let difficultyOptions = $derived(
		SINGLE_SEGMENT_CATEGORIES.find((category) => category.id === difficultyCategory)?.difficulties ?? []
	);
	let title = $derived(
		step === 'mode' ? 'Monitor source' : step === 'category' ? 'Choose single segment category' : 'Choose Any% difficulty'
	);
</script>

{#if open}
	<div class="obs-overlay fixed inset-0 z-50 flex items-center justify-center p-4">
		<button type="button" aria-label="Close run mode chooser" class="absolute inset-0 cursor-default" onclick={closeAll}></button>
		<dialog open aria-label="Choose monitor mode" class="obs-dialog relative z-10 m-0 w-full max-w-md overflow-hidden rounded p-0">
			<header class="obs-dialog-header px-4 py-3">
				<h2 class="obs-heading text-lg font-semibold">{title}</h2>
				<p class="obs-dim mt-1 font-mono text-xs">{sourceName ?? 'Selected source'}</p>
			</header>
			<div class="grid gap-3 p-4">
				{#if step === 'mode'}
					<button type="button" class="obs-list-button grid gap-1 rounded px-3 py-3 text-left disabled:cursor-not-allowed disabled:opacity-50" disabled={!clipsAvailable} onclick={() => choose('clips')}>
						<span class="obs-list-title text-sm font-semibold">Record clips</span>
						<span class="obs-list-detail font-mono text-xs">
							{clipsAvailable ? 'Detect individual stages and save replay-buffer clips.' : 'Replay buffer is unavailable.'}
						</span>
					</button>
					<button type="button" class="obs-list-button grid gap-1 rounded px-3 py-3 text-left" onclick={() => (step = 'category')}>
						<span class="obs-list-title text-sm font-semibold">Single segment run</span>
						<span class="obs-list-detail font-mono text-xs">Track real-time splits and record the whole segment.</span>
					</button>
				{:else if step === 'category'}
					{#each SINGLE_SEGMENT_CATEGORIES as category}
						<button type="button" class="obs-list-button grid gap-1 rounded px-3 py-3 text-left" onclick={() => selectCategory(category.id)}>
							<span class="obs-list-title text-sm font-semibold">{category.title}</span>
							<span class="obs-list-detail font-mono text-xs">{category.description}</span>
						</button>
					{/each}
				{:else}
					{#each difficultyOptions as difficulty}
						<button type="button" class="obs-list-button grid gap-1 rounded px-3 py-3 text-left" onclick={() => selectDifficulty(difficulty)}>
							<span class="obs-list-title text-sm font-semibold">{difficulty}</span>
							<span class="obs-list-detail font-mono text-xs">Run Any% on {difficulty}</span>
						</button>
					{/each}
				{/if}
				<div class="flex justify-between">
					{#if step !== 'mode'}
						<button type="button" class="obs-text-button px-2 py-1 font-mono text-xs" onclick={back}>back</button>
					{:else}
						<span></span>
					{/if}
					<button type="button" class="obs-text-button px-2 py-1 font-mono text-xs" onclick={closeAll}>close</button>
				</div>
			</div>
		</dialog>
	</div>
{/if}
