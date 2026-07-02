<script lang="ts">
	import { goto } from '$app/navigation';
	import { getSources, screenshotUrl, type ObsSource } from '../lib/api';
	import { replayBuffer, refreshReplayBuffer } from '$lib/replayBuffer.svelte';
	import WizardFrame from '$lib/wizard/WizardFrame.svelte';
	import OptionList, { type Option } from '$lib/wizard/OptionList.svelte';
	import { onMount } from 'svelte';

	let sources = $state<ObsSource[] | null>(null);
	let reloading = $state(false);
	let missingPreviewBySource = $state<Record<string, boolean>>({});
	// Bumped on each reload and woven into the screenshot URLs so the browser
	// re-fetches the previews (the URL is otherwise identical and would be cached).
	let previewVersion = $state(0);

	const MIN_REPLAY_BUFFER_SECONDS = 1100;
	const RECOMMENDED_REPLAY_BUFFER_SECONDS = 1200;

	// The replay buffer must be available to record; gate source selection on it.
	// Anything other than a confirmed "available" keeps selection disabled.
	const replayUnavailable = $derived(replayBuffer.status?.available !== true);
	const replayBufferTooShort = $derived(
		replayBuffer.status?.available === true &&
			replayBuffer.status.maxSeconds !== null &&
			replayBuffer.status.maxSeconds < MIN_REPLAY_BUFFER_SECONDS
	);

	// Re-query OBS for its current sources. Used both for the initial load and the
	// manual "refresh" button, so the list can be updated without a full page reload.
	const reload = async () => {
		reloading = true;
		try {
			sources = await getSources();
			previewVersion++;
			missingPreviewBySource = {};
			// Re-check the replay buffer too; the user may have just toggled it.
			await refreshReplayBuffer();
		} finally {
			reloading = false;
		}
	};
	onMount(() => {
		reload();
	});

	let options = $derived<Option[]>((sources ?? []).map((s) => ({ title: s.name, detail: s.id, key: s.name })));

	const select = (option: Option) => {
		if (replayUnavailable) return;
		goto(`/source/${encodeURIComponent(option.title)}`);
	};

	const previewKey = (option: Option): string => option.key ?? option.title;

	const markPreviewMissing = (key: string) => {
		missingPreviewBySource = { ...missingPreviewBySource, [key]: true };
	};
</script>

<svelte:head>
	<title>Setup</title>
</svelte:head>

<!-- A live preview of each source, fetched asynchronously by the browser so the
	user can recognise which capture is which. The frame is letterboxed on black to
	preserve aspect ratio; broken/uncaptured sources show a fixed-size placeholder. -->
{#snippet leading(option: Option)}
	{@const key = previewKey(option)}
	{#if missingPreviewBySource[key]}
		<div class="obs-preview-missing aspect-video max-h-36 w-full shrink-0 sm:h-36 sm:w-64">
			<span class="px-3 font-mono text-xs leading-snug">No image returned from OBS</span>
		</div>
	{:else}
		<img
			src="{screenshotUrl(option.title)}&v={previewVersion}"
			alt="Preview of {option.title}"
			loading="lazy"
			onerror={() => markPreviewMissing(key)}
			class="obs-preview aspect-video max-h-36 w-full shrink-0 object-contain sm:h-36 sm:w-auto"
		/>
	{/if}
{/snippet}

<WizardFrame
	step={1}
	title="Choose your capture source"
	subtitle="Pick the OBS source attached to your N64's video output."
>
	{#if replayBuffer.status && !replayBuffer.status.available}
		<div class="obs-alert-warning mb-4 rounded px-4 py-3">
			<p class="obs-alert-warning-title text-sm font-semibold">
				{replayBuffer.status.enabled ? 'Replay buffer is unavailable' : 'Replay buffer is disabled'}
			</p>
			<p class="obs-alert-warning-body mt-1 font-mono text-xs">
				{#if replayBuffer.status.enabled}
					OBS has Replay Buffer enabled, but the current Output settings do not expose a usable replay buffer. Lossless
					recording quality is one OBS setting that disables it. Change the Output settings, then refresh.
				{:else}
					Enable it in OBS under Settings → Output → Replay Buffer, then refresh.
				{/if}
				You can't pick a source until the replay buffer is usable.
			</p>
		</div>
	{/if}

	{#if replayBufferTooShort}
		<div class="obs-alert-warning mb-4 rounded px-4 py-3">
			<p class="obs-alert-warning-title text-sm font-semibold">Replay buffer time is short</p>
			<p class="obs-alert-warning-body mt-1 font-mono text-xs">
				OBS is configured for {replayBuffer.status?.maxSeconds} seconds. GoldenEye's in-game timer can reach 1023 seconds,
				and this tool recommends extra room for starting and ending cutscenes plus the mission report and stats screens. Set
				Maximum Replay Time to {RECOMMENDED_REPLAY_BUFFER_SECONDS} seconds for near-maximum-length runs.
			</p>
		</div>
	{/if}

	{#if sources === null}
		<p class="obs-dim font-mono text-sm">Loading sources…</p>
	{:else if sources.length === 0}
		<div class="obs-empty-state rounded px-4 py-6 text-center">
			<p class="obs-muted text-sm">No OBS sources found.</p>
			<p class="obs-dim mt-1 font-mono text-xs">
				Add a video capture source in OBS, then reload the page or refresh OBS sources.
			</p>
		</div>
	{:else}
		<OptionList {options} onSelect={select} {leading} disabled={replayUnavailable} />
	{/if}

	{#if sources !== null}
		<div class="mt-6 flex justify-center">
			<button
				type="button"
				onclick={reload}
				disabled={reloading}
				class="obs-text-button px-2 py-1 font-mono text-xs underline-offset-2"
			>
				{reloading ? 'refreshing…' : 'refresh OBS sources'}
			</button>
		</div>
	{/if}
</WizardFrame>
