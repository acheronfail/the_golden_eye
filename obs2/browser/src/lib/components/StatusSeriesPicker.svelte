<script lang="ts">
	import type { RunStatus } from '$lib/api';
	import { ALL_STATUSES, STATUS_LABELS } from '$lib/utils/statisticsView';

	let { value = $bindable() }: { value: RunStatus[] } = $props();

	function toggle(status: RunStatus) {
		if (value.includes(status)) {
			if (value.length > 1) value = value.filter((candidate) => candidate !== status);
		} else {
			value = [...value, status];
		}
	}
</script>

<fieldset class="flex flex-wrap gap-x-4 gap-y-2">
	<legend class="sr-only">Visible run statuses</legend>
	{#each ALL_STATUSES as status}
		<label class="flex items-center gap-2 text-sm obs-muted">
			<input
				class="obs-checkbox"
				type="checkbox"
				checked={value.includes(status)}
				disabled={value.length === 1 && value.includes(status)}
				onchange={() => toggle(status)}
				aria-label={`Show ${STATUS_LABELS[status]} runs`}
			/>
			{STATUS_LABELS[status]}
		</label>
	{/each}
</fieldset>
