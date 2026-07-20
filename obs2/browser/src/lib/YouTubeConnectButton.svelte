<script lang="ts">
	import { youtube } from '$lib/youtube.svelte';

	// Shared Connect/Cancel control. While the OAuth flow is pending the button
	// flips to a danger-styled Cancel that resets the flow, so a user whose
	// browser never opened (or was closed) can retry without refreshing OBS.
	let { class: className = 'px-4 py-2 font-mono text-sm' }: { class?: string } = $props();

	const connect = () => {
		void youtube.connect().catch((err) => console.warn('Failed to connect YouTube', err));
	};
	const cancel = () => {
		void youtube.cancel().catch((err) => console.warn('Failed to cancel YouTube connect', err));
	};
</script>

{#if youtube.connecting}
	<button type="button" class="obs-button obs-button-danger {className}" onclick={cancel}> Cancel </button>
{:else}
	<button
		type="button"
		class="obs-button obs-button-gold {className}"
		disabled={!youtube.oauthConfigured}
		onclick={connect}
	>
		Connect YouTube
	</button>
{/if}
