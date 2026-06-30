<script lang="ts" module>
	export interface Option {
		/** Primary label, shown prominently. */
		title: string;
		/** Optional secondary detail, rendered monospace beneath the title. */
		detail?: string;
		/** Stable key for the `{#each}` block. Defaults to `title`. */
		key?: string;
	}
</script>

<script lang="ts">
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
		<li>
			<button
				bind:this={items[i]}
				type="button"
				{disabled}
				onclick={() => onSelect(option, i)}
				onkeydown={(e) => onkeydown(e, i)}
				class="group flex w-full items-center gap-4 rounded-md border border-amber-700 bg-neutral-950/60 px-4 py-3 text-left transition-colors
					hover:border-amber-400 hover:bg-amber-600 hover:text-black hover:cursor-pointer
					focus:outline-none focus-visible:border-amber-400 focus-visible:ring-2 focus-visible:ring-amber-400
					disabled:pointer-events-none disabled:opacity-40"
			>
				{#if leading}
					{@render leading(option, i)}
				{/if}
				<span class="flex min-w-0 flex-col">
					<span class="truncate font-medium text-amber-300 group-hover:text-black">
						{option.title}
					</span>
					{#if option.detail}
						<span class="truncate font-mono text-xs text-neutral-400 group-hover:text-black/70">
							{option.detail}
						</span>
					{/if}
				</span>
				<span
					class="ml-auto translate-x-0 font-mono text-amber-500 transition-transform group-hover:translate-x-1 group-hover:text-black"
					aria-hidden="true"
				>
					→
				</span>
			</button>
		</li>
	{/each}
</ul>
