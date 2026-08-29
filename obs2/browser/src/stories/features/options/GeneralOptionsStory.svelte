<script lang="ts">
	import OptionsGeneral from '$lib/features/options/OptionsGeneral.svelte';
	import { settings } from '$lib/stores/settings.svelte';

	let { autoStartMonitorOnLaunch = false }: { autoStartMonitorOnLaunch?: boolean } = $props();

	$effect(() => {
		settings.values.autoStartMonitorOnLaunch = autoStartMonitorOnLaunch;
	});
</script>

<main class="mx-auto grid w-full max-w-2xl gap-4 px-4 py-8 sm:px-6">
	<div>
		<h1 class="text-2xl font-semibold obs-heading">Options</h1>
		<p class="mt-1 text-sm obs-subtitle">General plugin behavior.</p>
	</div>
	<OptionsGeneral />
</main>
