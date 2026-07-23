<script lang="ts">
	import type { Snippet } from 'svelte';

	let {
		id,
		title,
		children,
		actions,
		bodyClass = 'grid gap-3 px-4 py-4 text-sm leading-6'
	}: {
		id: string;
		title: string;
		children: Snippet;
		actions: Snippet;
		bodyClass?: string;
	} = $props();
</script>

<div class="obs-overlay fixed inset-0 z-50 flex items-center justify-center p-4" role="presentation">
	<div
		class="obs-dialog w-full max-w-md overflow-hidden rounded"
		role="dialog"
		aria-modal="true"
		aria-labelledby={`${id}-title`}
		aria-describedby={`${id}-body`}
	>
		<div class="obs-dialog-header px-4 py-3">
			<h2 id={`${id}-title`} class="obs-heading text-lg font-semibold">{title}</h2>
		</div>
		<div id={`${id}-body`} class={bodyClass}>
			{@render children()}
		</div>
		<div class="flex justify-end gap-2 px-4 pb-4">
			{@render actions()}
		</div>
	</div>
</div>
