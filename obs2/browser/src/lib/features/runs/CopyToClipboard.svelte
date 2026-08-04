<script lang="ts">
	import { onDestroy } from 'svelte';

	let {
		url,
		onOpen
	}: {
		url: string;
		onOpen?: (url: string) => void;
	} = $props();

	let copied = $state(false);

	let copyResetTimer: ReturnType<typeof setTimeout> | null = null;

	const copyUrl = () => {
		if (!url) return;
		void navigator.clipboard
			.writeText(url)
			.then(() => {
				copied = true;
				if (copyResetTimer) clearTimeout(copyResetTimer);
				copyResetTimer = setTimeout(() => {
					copied = false;
					copyResetTimer = null;
				}, 1500);
			})
			.catch((err) => console.warn('Failed to copy URL', err));
	};

	const selectUrl = (event: Event) => {
		(event.currentTarget as HTMLInputElement).select();
	};

	onDestroy(() => {
		if (copyResetTimer) clearTimeout(copyResetTimer);
	});
</script>

<div class="flex w-full items-center justify-center gap-2 px-2 sm:px-8">
	<input
		class="obs-input min-w-0 flex-1 truncate border-(--obs-border-soft) px-3 py-1.5 text-center font-mono text-xs shadow-[inset_0_1px_0_var(--obs-border-soft)]"
		readonly
		value={url}
		aria-label="URL"
		onclick={selectUrl}
		onfocus={selectUrl}
	/>
	<button type="button" class="obs-button w-17 obs-button-xs" onclick={copyUrl}>{copied ? 'Copied' : 'Copy'}</button>
	{#if onOpen}
		<button type="button" class="obs-button obs-button-xs" onclick={() => onOpen(url)}>Open</button>
	{/if}
</div>
