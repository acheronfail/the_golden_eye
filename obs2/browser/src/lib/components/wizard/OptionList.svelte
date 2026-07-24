<script lang="ts" module>
	export interface Option {
		/** Primary label, shown prominently. */
		title: string;
		/** Optional group heading, rendered when it changes between rows. */
		section?: string;
		/** Optional secondary detail, rendered monospace beneath the title. */
		detail?: string;
		/** Stable key for the `{#each}` block. Defaults to `title`. */
		key?: string;
		/** Optional visual tone for domain-specific list rows. */
		tone?: 'success';
	}
</script>

<script lang="ts">
	import SectionTitle from '$lib/components/SectionTitle.svelte';
	import type { Snippet } from 'svelte';

	let {
		options,
		onSelect,
		leading,
		disabled = false
	}: {
		options: Option[];
		/** Invoked with the chosen option (and its index) on click/Enter/Space. */
		onSelect: (option: Option, index: number) => void;
		/** Optional visual rendered at the start of each row (e.g. a thumbnail).
		 * Receives the option and its index. */
		leading?: Snippet<[Option, number]>;
		/** When true, every option is non-interactive (greyed out, not focusable).
		 * Used to gate selection until a prerequisite is met. */
		disabled?: boolean;
	} = $props();

	let items = $state<HTMLButtonElement[]>([]);

	// Roving focus: arrow keys move between options, Home/End jump to the ends.
	// Enter/Space activate natively (these are real <button>s).
	const onkeydown = (event: KeyboardEvent, index: number) => {
		let next: number | null = null;
		switch (event.key) {
			case 'ArrowDown':
				next = (index + 1) % options.length;
				break;
			case 'ArrowUp':
				next = (index - 1 + options.length) % options.length;
				break;
			case 'Home':
				next = 0;
				break;
			case 'End':
				next = options.length - 1;
				break;
		}
		if (next !== null) {
			event.preventDefault();
			items[next]?.focus();
		}
	};

	// Focus the first option on mount so the keyboard works without a click first
	// (skipped while disabled — the options aren't focusable then).
	$effect(() => {
		if (!disabled) items[0]?.focus();
	});
</script>

<ul class="flex flex-col gap-3">
	{#each options as option, i (option.key ?? option.title)}
		{#if option.section && option.section !== options[i - 1]?.section}
			<li class:mt-2={i > 0}>
				<SectionTitle title={option.section} />
			</li>
		{/if}
		<li>
			<button
				bind:this={items[i]}
				type="button"
				{disabled}
				onclick={() => onSelect(option, i)}
				onkeydown={(e) => onkeydown(e, i)}
				class="group obs-list-button flex w-full flex-col items-stretch gap-3 rounded px-4 py-3 text-left transition-colors sm:flex-row sm:items-center sm:gap-4"
				class:obs-list-button-success={option.tone === 'success'}
			>
				{#if leading}
					{@render leading(option, i)}
				{/if}
				<span class="flex min-w-0 items-center gap-3">
					<span class="flex min-w-0 flex-1 flex-col">
						<span class="block min-w-0 font-medium wrap-break-word obs-list-title sm:truncate">
							{option.title}
						</span>
						{#if option.detail}
							<span class="block min-w-0 font-mono text-xs wrap-break-word obs-list-detail sm:truncate">
								{option.detail}
							</span>
						{/if}
					</span>
					<span
						class="ml-auto shrink-0 translate-x-0 font-mono obs-list-arrow transition-transform group-hover:translate-x-1"
						aria-hidden="true"
					>
						→
					</span>
				</span>
			</button>
		</li>
	{/each}
</ul>
