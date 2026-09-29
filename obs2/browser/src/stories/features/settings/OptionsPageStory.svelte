<script lang="ts">
	import AppHeader from '$lib/app/AppHeader.svelte';
	import type { SettingsSection } from '$lib/features/settings/SettingsSectionTabs.svelte';
	import { settings } from '$lib/stores/settings.svelte';
	import OptionsPage from '../../../routes/(app)/options/+page.svelte';

	let { section = 'general' }: { section?: SettingsSection } = $props();

	$effect.pre(() => {
		localStorage.setItem('the-golden-eye.options-tab', section);
	});
	settings.loaded = true;
	settings.configPath = '/Users/bond/Library/Application Support/The Golden Eye/settings.json';
</script>

<div class="obs-app-shell flex h-screen min-h-0 min-w-100 flex-col overflow-hidden">
	<AppHeader
		links={[
			{ href: '/', label: 'Monitor' },
			{ href: '/statistics', label: 'Statistics' },
			{ href: '/runs', label: 'Runs' },
			{ href: '/options', label: 'Options' }
		]}
		currentPath="/options"
		pluginVersion="2.4.0"
	/>
	<div class="min-h-0 flex-1 overflow-x-hidden overflow-y-auto obs-content-scroller">
		<OptionsPage />
	</div>
</div>
