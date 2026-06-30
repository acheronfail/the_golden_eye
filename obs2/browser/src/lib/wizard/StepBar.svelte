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

<ol class="mb-10 flex items-center gap-2 font-mono text-xs tracking-wide">
	{#each STEPS as label, i}
		{@const step = i + 1}
		{@const done = step < current}
		{@const active = step === current}
		{@const href = done ? hrefs[i] : undefined}
		<li class="flex items-center gap-2">
			<svelte:element
				this={href ? 'a' : 'span'}
				{href}
				class="flex items-center gap-2 rounded border px-2.5 py-1
					{active
					? 'border-amber-400 bg-amber-600 text-black'
					: done
						? 'border-amber-700 text-amber-500'
						: 'border-neutral-700 text-neutral-500'}
					{href ? 'transition-colors hover:border-amber-400 hover:text-amber-300' : ''}"
			>
				<span
					class="flex h-4 w-4 items-center justify-center rounded-full border text-[0.625rem]
						{active ? 'border-black' : done ? 'border-amber-500' : 'border-neutral-600'}"
				>
					{done ? '✓' : step}
				</span>
				{label}
			</svelte:element>
			{#if step < STEPS.length}
				<span class="text-neutral-700" aria-hidden="true">›</span>
			{/if}
		</li>
	{/each}
</ol>
