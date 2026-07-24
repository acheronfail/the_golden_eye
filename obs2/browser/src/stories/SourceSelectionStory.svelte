<script lang="ts">
	import SourcePreview from '$lib/components/SourcePreview.svelte';
	import OptionList, { type Option } from '$lib/components/wizard/OptionList.svelte';
	import WizardFrame from '$lib/components/wizard/WizardFrame.svelte';

	let {
		state = 'available',
		previews = true,
		disabled = false,
		lastUsed = true
	}: {
		state?: 'loading' | 'empty' | 'available' | 'missing' | 'mixed';
		previews?: boolean;
		disabled?: boolean;
		lastUsed?: boolean;
	} = $props();

	const sources = [
		{ title: 'Nintendo 64', detail: 'av_capture_input', key: 'n64' },
		{ title: 'Capture Card 2', detail: 'decklink_input', key: 'capture-2' }
	];
	const options = $derived<Option[]>(
		sources.map((source, index) => ({
			...source,
			section: lastUsed && index === 0 ? 'last used source' : 'sources'
		}))
	);
	const previewImage =
		'data:image/svg+xml;charset=utf-8,' +
		encodeURIComponent(
			`<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 640 360"><defs><linearGradient id="g" x2="1" y2="1"><stop stop-color="#17233d"/><stop offset="1" stop-color="#08100b"/></linearGradient></defs><rect width="640" height="360" fill="url(#g)"/><circle cx="320" cy="160" r="86" fill="#d7a416" opacity=".78"/><text x="320" y="280" text-anchor="middle" fill="#f1f2f5" font-family="monospace" font-size="28">GOLDENEYE 007</text></svg>`
		);
</script>

{#snippet leading(option: Option, index: number)}
	<SourcePreview
		src={previewImage}
		alt="Preview of {option.title}"
		missing={state === 'missing' || (state === 'mixed' && index === 1)}
	/>
{/snippet}

<WizardFrame title="Choose your capture source" subtitle="Pick the OBS source attached to your N64's video output.">
	{#if disabled}
		<div class="mb-4 rounded obs-alert-warning px-4 py-3">
			<p class="text-sm font-semibold obs-alert-warning-title">Replay buffer is disabled</p>
			<p class="mt-1 font-mono text-xs obs-alert-warning-body">
				Enable it in OBS under Settings → Output → Replay Buffer. You can't pick a source until it is usable.
			</p>
		</div>
	{/if}

	{#if state === 'loading'}
		<p class="font-mono text-sm obs-dim">Loading sources…</p>
	{:else if state === 'empty'}
		<div class="rounded obs-empty-state px-4 py-6 text-center">
			<p class="text-sm obs-muted">No OBS sources found.</p>
			<p class="mt-1 font-mono text-xs obs-dim">Add a video capture source in OBS.</p>
		</div>
	{:else}
		<div class="mb-2 flex justify-end">
			<button type="button" class="obs-text-button obs-button-xs">{previews ? 'Hide previews' : 'Show previews'}</button
			>
		</div>
		<OptionList {options} onSelect={() => {}} leading={previews ? leading : undefined} {disabled} />
	{/if}
</WizardFrame>
