<script lang="ts">
	import ResetSettingsDialog from '$lib/components/ResetSettingsDialog.svelte';
	import RunsFolderChooser from '$lib/components/RunsFolderChooser.svelte';
	import WelcomeDialog from '$lib/components/WelcomeDialog.svelte';
	import { completedDirectory, failedDirectory } from './fixtures';

	let {
		kind,
		busy = false,
		error = null
	}: {
		kind: 'welcome' | 'reset' | 'folders';
		busy?: boolean;
		error?: string | null;
	} = $props();
	const noop = () => {};
</script>

{#if kind === 'welcome'}
	<WelcomeDialog dismiss={noop} />
{:else if kind === 'reset'}
	<ResetSettingsDialog {busy} {error} cancel={noop} reset={noop} />
{:else}
	<RunsFolderChooser open {busy} {completedDirectory} {failedDirectory} close={noop} reveal={noop} />
{/if}
