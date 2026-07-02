<script lang="ts">
	/** The ordered steps of the setup flow. Shared by every wizard screen so the
	 * progress indicator stays consistent. `current` is 1-based. */
	export const STEPS = ['Source', 'Language', 'Monitor'] as const;

	let {
		current,
		/** Optional destinations aligned to STEPS (by 1-based step index). Only the
		 * entries for already-completed steps are turned into links; the current and
		 * upcoming steps are never clickable. */
		hrefs = []
	}: { current: number; hrefs?: (string | undefined)[] } = $props();
</script>

<ol class="mb-10 flex flex-wrap items-center gap-2 font-mono text-xs tracking-wide">
	{#each STEPS as label, i}
		{@const step = i + 1}
		{@const done = step < current}
		{@const active = step === current}
		{@const href = done ? hrefs[i] : undefined}
		<li class="flex items-center gap-2">
			<svelte:element
				this={href ? 'a' : 'span'}
				{href}
				class="obs-step flex items-center gap-2 rounded px-2.5 py-1
					{active ? 'obs-step-active' : done ? 'obs-step-done' : ''}
					{href ? 'obs-step-interactive transition-colors' : ''}"
			>
				<span class="obs-step-marker flex h-4 w-4 items-center justify-center rounded-full border text-[0.625rem]">
					{done ? '✓' : step}
				</span>
				{label}
			</svelte:element>
			{#if step < STEPS.length}
				<span class="obs-step-divider" aria-hidden="true">›</span>
			{/if}
		</li>
	{/each}
</ol>
