<script lang="ts">
	import { goto } from '$app/navigation';
	import { screenshotUrl, type MonitorRunMode } from '../lib/api';
	import { refreshReplayBuffer, replayBuffer } from '$lib/replayBuffer.svelte';
	import { obsSources } from '$lib/sources.svelte';
	import WizardFrame from '$lib/wizard/WizardFrame.svelte';
	import OptionList, { type Option } from '$lib/wizard/OptionList.svelte';
	import RunModeChooser from '$lib/RunModeChooser.svelte';
	import { runModePath } from '$lib/singleSegment';
	import { onMount } from 'svelte';

	let missingPreviewBySource = $state<Record<string, boolean>>({});
	let lastPreviewVersion = $state(0);
	let previewTick = $state(0);
	let selectedSource = $state<string | null>(null);

	const MIN_REPLAY_BUFFER_SECONDS = 1100;
	const RECOMMENDED_REPLAY_BUFFER_SECONDS = 1200;
	const sources = $derived(obsSources.items);
	const previewVersion = $derived(`${obsSources.version}-${previewTick}`);

	// Clip recording needs the replay buffer; single-segment recording uses OBS recording.
	const replayUnavailable = $derived(replayBuffer.status?.available !== true);
	const replayBufferTooShort = $derived(
		replayBuffer.status?.available === true &&
			replayBuffer.status.maxSeconds !== null &&
			replayBuffer.status.maxSeconds < MIN_REPLAY_BUFFER_SECONDS
	);

	$effect(() => {
		if (obsSources.version !== lastPreviewVersion) {
			lastPreviewVersion = obsSources.version;
			missingPreviewBySource = {};
		}
	});

	onMount(() => {
		let replayRefreshInFlight = false;
		const timer = window.setInterval(() => {
			previewTick += 1;
			if (!replayRefreshInFlight) {
				replayRefreshInFlight = true;
				refreshReplayBuffer().finally(() => {
					replayRefreshInFlight = false;
				});
			}
		}, 2000);

		return () => {
			window.clearInterval(timer);
		};
	});

	let options = $derived<Option[]>((sources ?? []).map((s) => ({ title: s.name, detail: s.id, key: s.name })));

	const select = (option: Option) => {
		selectedSource = option.title;
	};

	const closeModeChooser = () => {
		selectedSource = null;
	};

	const chooseMode = (mode: MonitorRunMode, difficulty?: number) => {
		if (!selectedSource) return;
		const query = difficulty === undefined ? '' : `?difficulty=${difficulty}`;
		goto(`/sources/${encodeURIComponent(selectedSource)}/${runModePath(mode)}${query}`);
		selectedSource = null;
	};

	const previewKey = (option: Option): string => option.key ?? option.title;

	const markPreviewMissing = (key: string) => {
		missingPreviewBySource = { ...missingPreviewBySource, [key]: true };
	};

	const markPreviewAvailable = (key: string) => {
		if (!missingPreviewBySource[key]) return;
		const next = { ...missingPreviewBySource };
		delete next[key];
		missingPreviewBySource = next;
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
			<img
				src="{screenshotUrl(option.title)}&v={previewVersion}"
				alt=""
				aria-hidden="true"
				class="hidden"
				onload={() => markPreviewAvailable(key)}
			/>
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

<WizardFrame title="Choose your capture source" subtitle="Pick the OBS source attached to your N64's video output.">
	{#if replayBuffer.status && !replayBuffer.status.available}
		<div class="obs-alert-warning mb-4 rounded px-4 py-3">
			<p class="obs-alert-warning-title text-sm font-semibold">
				{replayBuffer.status.enabled ? 'Replay buffer is unavailable' : 'Replay buffer is disabled'}
			</p>
			<p class="obs-alert-warning-body mt-1 font-mono text-xs">
				{#if replayBuffer.status.enabled}
					OBS has Replay Buffer enabled, but the current Output settings do not expose a usable replay buffer. Lossless
					recording quality is one OBS setting that disables it. Change the Output settings, then reopen this page if
					the status has not changed.
				{:else}
					Enable it in OBS under Settings → Output → Replay Buffer.
				{/if}
				Record Clips mode needs a usable replay buffer. Single segment runs can still use OBS recording.
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
			<p class="obs-dim mt-1 font-mono text-xs">Add a video capture source in OBS.</p>
		</div>
	{:else}
		<OptionList {options} onSelect={select} {leading} />
	{/if}
</WizardFrame>

<RunModeChooser
	open={selectedSource !== null}
	sourceName={selectedSource}
	clipsAvailable={!replayUnavailable}
	close={closeModeChooser}
	choose={chooseMode}
/>
