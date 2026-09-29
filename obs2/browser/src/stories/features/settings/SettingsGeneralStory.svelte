<script lang="ts">
	import SettingsGeneral from '$lib/features/settings/SettingsGeneral.svelte';
	import { settings } from '$lib/stores/settings.svelte';

	let { includePrereleases = false }: { includePrereleases?: boolean } = $props();

	$effect(() => {
		settings.values.includePrereleases = includePrereleases;
	});
</script>

<main class="mx-auto grid w-full max-w-2xl gap-4 px-4 py-8 sm:px-6">
	<SettingsGeneral />
</main>
