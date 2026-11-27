<script lang="ts">
	import { backend } from '$lib/api';
	import StreamStopDialog from '$lib/features/monitor/StreamStopDialog.svelte';
	import { streamStopPrompt } from '$lib/features/monitor/streamStopPrompt.svelte';
	import './layout.css';
	let { children } = $props();

	const dismissStreamStop = () => (streamStopPrompt.open = false);
</script>

<div class="contents" inert={streamStopPrompt.open}>
	{@render children()}
</div>

{#if streamStopPrompt.open}
	<StreamStopDialog onClose={dismissStreamStop} stopStream={() => backend.stopStream()} />
{/if}
