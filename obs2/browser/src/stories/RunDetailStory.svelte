<script lang="ts">
	import type { EditableRunMetadata, RunClip, YouTubeStatus } from '$lib/api';
	import RunDetailDialog from '$lib/components/RunDetailDialog.svelte';
	import { youtube } from '$lib/stores/youtube.svelte';
	import { LEVEL_OPTIONS, type RunDetailView } from '$lib/utils/runsView';
	import { draftForRun } from './fixtures';

	let {
		clip,
		status,
		connecting = false,
		error = null,
		modalBusy = null,
		modalError = null
	}: {
		clip: RunClip;
		status: YouTubeStatus;
		connecting?: boolean;
		error?: string | null;
		modalBusy?: string | null;
		modalError?: string | null;
	} = $props();

	let metadataDraft = $state<EditableRunMetadata | null>(null);
	let draftPath = $state('');
	const noop = () => {};
	let view = $derived<RunDetailView>({
		modal: { error: modalError, busy: modalBusy },
		display: {
			fileBrowserLabel: 'show in finder',
			levelOptions: LEVEL_OPTIONS.map((level) => ({ value: level, label: level }))
		},
		actions: {
			close: noop,
			delete: noop,
			reveal: noop,
			rename: noop,
			saveMetadata: noop,
			normalizeDraftTime: noop
		}
	});

	$effect(() => {
		if (clip.path !== draftPath) {
			draftPath = clip.path;
			metadataDraft = draftForRun(clip);
		}
		youtube.applyStatus(status);
		youtube.connecting = connecting;
		youtube.cancelling = false;
		youtube.disconnecting = false;
		youtube.error = error;
	});
</script>

<RunDetailDialog {clip} bind:metadataDraft {view} />
