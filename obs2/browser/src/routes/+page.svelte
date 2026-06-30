<script lang="ts">
	import { goto } from '$app/navigation';
	import { getSources, screenshotUrl, type ObsSource } from '../lib/api';
	import WizardFrame from '$lib/wizard/WizardFrame.svelte';
	import OptionList, { type Option } from '$lib/wizard/OptionList.svelte';

	let sources = $state<ObsSource[] | null>(null);
	let reloading = $state(false);
	// Bumped on each reload and woven into the screenshot URLs so the browser
	// re-fetches the previews (the URL is otherwise identical and would be cached).
	let previewVersion = $state(0);

	// Re-query OBS for its current sources. Used both for the initial load and the
	// manual "refresh" button, so the list can be updated without a full page reload.
	const reload = async () => {
		reloading = true;
		try {
			sources = await getSources();
			previewVersion++;
		} finally {
			reloading = false;
		}
	};
	reload();

	let options = $derived<Option[]>(
		(sources ?? []).map((s) => ({ title: s.name, detail: s.id, key: s.name }))
	);

	const select = (option: Option) => goto(`/source/${encodeURIComponent(option.title)}`);
</script>

<svelte:head>
	<title>Setup</title>
</svelte:head>

<!-- A live preview of each source, fetched asynchronously by the browser so the
	user can recognise which capture is which. The frame is letterboxed on black to
	preserve aspect ratio, and broken/uncaptured sources just hide the image. -->
{#snippet leading(option: Option)}
	<img
		src="{screenshotUrl(option.title)}&v={previewVersion}"
		alt="Preview of {option.title}"
		loading="lazy"
		onerror={(e) => ((e.currentTarget as HTMLImageElement).style.visibility = 'hidden')}
		class="h-36 shrink-0 border border-slate-600 bg-black object-contain"
	/>
{/snippet}

<WizardFrame
	step={1}
	title="Choose your capture source"
	subtitle="Pick the OBS source attached to your N64's video output."
>
	{#if sources === null}
		<p class="font-mono text-sm text-neutral-500">Loading sources…</p>
	{:else if sources.length === 0}
		<div class="rounded-md border border-neutral-700 bg-neutral-950/60 px-4 py-6 text-center">
			<p class="text-sm text-neutral-300">No OBS sources found.</p>
			<p class="mt-1 font-mono text-xs text-neutral-500">
				Add a video capture source in OBS, then reload the page or refresh OBS sources.
			</p>
		</div>
	{:else}
		<OptionList {options} onSelect={select} {leading} />
	{/if}

	{#if sources !== null}
		<div class="mt-6 flex justify-center">
			<button
				type="button"
				onclick={reload}
				disabled={reloading}
				class="font-mono border border-neutral-800 rounded px-2 py-1 text-xs text-neutral-500 underline-offset-2 transition-colors hover:text-amber-300 hover:border-amber-300 hover:cursor-pointer disabled:text-neutral-700 disabled:no-underline"
			>
				{reloading ? 'refreshing…' : 'refresh OBS sources'}
			</button>
		</div>
	{/if}
</WizardFrame>
