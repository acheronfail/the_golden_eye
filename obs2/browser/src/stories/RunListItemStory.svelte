<script lang="ts">
	import type { RunClip } from '$lib/api';
	import RunListItem from '$lib/components/RunListItem.svelte';

	let {
		clip,
		busy = false,
		menuOpen = $bindable(false)
	}: { clip: RunClip; busy?: boolean; menuOpen?: boolean } = $props();
</script>

<main class="mx-auto w-full max-w-3xl px-3 py-6 sm:px-4">
	<RunListItem
		{clip}
		{busy}
		bind:menuOpen
		fileBrowserLabel="Show in file browser"
		open={() => {}}
		rename={() => {}}
		reveal={() => {}}
		remove={() => {}}
	/>
</main>
