<script lang="ts">
	import ModalDialog from '$lib/components/ModalDialog.svelte';
	import ReplayBufferStopDialog from '$lib/components/ReplayBufferStopDialog.svelte';
	import ResetSettingsDialog from '$lib/components/ResetSettingsDialog.svelte';
	import WelcomeDialog from '$lib/components/WelcomeDialog.svelte';

	let {
		kind,
		busy = false,
		error = null
	}: {
		kind: 'modal-dialog' | 'replay-buffer' | 'welcome' | 'reset';
		busy?: boolean;
		error?: string | null;
	} = $props();
	const noop = () => {};
</script>

{#if kind === 'modal-dialog'}
	<ModalDialog id="storybook-modal-dialog" title="Shared modal dialog">
		<p>This story exercises the shared frame used for focused choices throughout the plugin.</p>
		<p class="obs-dim">Dialog content and actions are supplied as reusable snippets.</p>

		{#snippet actions()}
			<button type="button" class="obs-button px-4 py-2">Secondary action</button>
			<button type="button" class="obs-button obs-button-gold px-4 py-2">Primary action</button>
		{/snippet}
	</ModalDialog>
{:else if kind === 'replay-buffer'}
	<ReplayBufferStopDialog {busy} {error} choose={noop} />
{:else if kind === 'welcome'}
	<WelcomeDialog dismiss={noop} />
{:else if kind === 'reset'}
	<ResetSettingsDialog {busy} {error} cancel={noop} reset={noop} />
{/if}
