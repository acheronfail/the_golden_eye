<script lang="ts">
	import SettingsSectionTabs, { type SettingsSection } from '$lib/features/settings/SettingsSectionTabs.svelte';

	let {
		initialSection = 'general',
		includeYouTube = false
	}: {
		initialSection?: SettingsSection;
		includeYouTube?: boolean;
	} = $props();

	let section = $state<SettingsSection>('general');

	$effect(() => {
		section = initialSection;
	});
</script>

<main class="mx-auto w-full max-w-2xl px-4 py-8 sm:px-6">
	<SettingsSectionTabs value={section} {includeYouTube} onChange={(value) => (section = value)} />
</main>
