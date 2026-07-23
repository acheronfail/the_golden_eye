<script lang="ts">
	import ResetSettingsDialog from '$lib/components/ResetSettingsDialog.svelte';
	import WelcomeDialog from '$lib/components/WelcomeDialog.svelte';

	let {
		kind,
		busy = false,
		error = null
	}: {
		kind: 'welcome' | 'reset';
		busy?: boolean;
		error?: string | null;
	} = $props();
	const noop = () => {};
</script>

{#if kind === 'welcome'}
	<WelcomeDialog dismiss={noop} />
{:else if kind === 'reset'}
	<ResetSettingsDialog {busy} {error} cancel={noop} reset={noop} />
{/if}
