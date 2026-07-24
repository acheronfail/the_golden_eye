<script lang="ts">
	import Select from './Select.svelte';
	import type { DifficultyNumber } from '$lib/api';
	import { DIFFICULTY_LABELS } from '$lib/api';
	import { LEVEL_NAMES } from '$lib/utils/statisticsView';

	let {
		levelNumber = $bindable(),
		difficultyNumber = $bindable()
	}: {
		levelNumber: number;
		difficultyNumber: DifficultyNumber;
	} = $props();

	const levelOptions = LEVEL_NAMES.map((label, index) => ({ value: String(index + 1), label }));
	const difficultyOptions = ([0, 1, 2, 3] as DifficultyNumber[]).map((value) => ({
		value: String(value),
		label: DIFFICULTY_LABELS[value]
	}));
</script>

<div class="flex flex-wrap items-end gap-3">
	<label class="grid min-w-40 flex-1 gap-1 text-xs font-semibold obs-muted" for="statistics-level">
		Level
		<Select
			id="statistics-level"
			value={String(levelNumber)}
			options={levelOptions}
			onChange={(value) => (levelNumber = Number(value))}
			class="px-3 py-2 text-left text-sm"
		/>
	</label>
	<label class="grid min-w-40 flex-1 gap-1 text-xs font-semibold obs-muted" for="statistics-difficulty">
		Difficulty
		<Select
			id="statistics-difficulty"
			value={String(difficultyNumber)}
			options={difficultyOptions}
			onChange={(value) => (difficultyNumber = Number(value) as DifficultyNumber)}
			class="px-3 py-2 text-left text-sm"
		/>
	</label>
</div>
