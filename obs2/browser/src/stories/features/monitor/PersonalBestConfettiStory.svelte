<script lang="ts">
	import { onMount } from 'svelte';
	import PersonalBestConfetti from '$lib/features/monitor/PersonalBestConfetti/PersonalBestConfetti.svelte';
	import type { MonitorDesign } from '$lib/features/monitor/monitorView';
	import MonitorPersonalBestStory from './MonitorPersonalBestStory.svelte';

	let {
		design = 'mission-glass',
		durationMs = 4000,
		autoplay = true,
		reducedMotion
	}: { design?: MonitorDesign; durationMs?: number; autoplay?: boolean; reducedMotion?: boolean } = $props();
	let trigger = $state(0);
	onMount(() => {
		if (autoplay) trigger = 1;
	});
</script>

<MonitorPersonalBestStory {design} scenario="stats" />
<PersonalBestConfetti {trigger} {durationMs} {reducedMotion} />
<button
	type="button"
	class="fixed right-6 bottom-6 z-[70] obs-button obs-button-gold px-4 py-2 text-sm"
	onclick={() => {
		trigger += 1;
	}}
>
	Replay PB confetti
</button>
