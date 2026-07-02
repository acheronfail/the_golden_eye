<script lang="ts">
	import { getRuns, runThumbnailUrl, runVideoUrl, type RunClip, type RunsResponse } from '$lib/api';
	import OptionList, { type Option } from '$lib/wizard/OptionList.svelte';
	import { onMount } from 'svelte';

	let runs = $state<RunsResponse | null>(null);
	let loading = $state(false);
	let error = $state<string | null>(null);
	let selected = $state<RunClip | null>(null);
	let previewVersion = $state(0);

	const clips = $derived(runs?.clips ?? []);
	const clipByPath = $derived(new Map(clips.map((clip) => [clip.path, clip])));
	const options = $derived<Option[]>(
		clips.map((clip) => ({
			title: clip.fileName,
			detail: runDetail(clip),
			key: clip.path
		}))
	);
	const directoryErrors = $derived((runs?.directories ?? []).filter((dir) => dir.error));

	const reload = async () => {
		loading = true;
		error = null;
		try {
			runs = await getRuns();
			previewVersion++;
		} catch (err) {
			error = err instanceof Error ? err.message : String(err);
		} finally {
			loading = false;
		}
	};

	onMount(() => {
		reload();
	});

	const select = (option: Option) => {
		selected = clipByPath.get(option.key ?? option.title) ?? null;
	};

	const close = () => {
		selected = null;
	};

	const onkeydown = (event: KeyboardEvent) => {
		if (selected && event.key === 'Escape') close();
	};

	function runDetail(clip: RunClip): string {
		const parts = [
			levelLabel(clip),
			romLanguageLabel(clip.metadata.romLanguage),
			clip.metadata.time,
			clip.metadata.difficulty,
			statusLabel(clip.metadata.status),
			formatDate(clip.metadata.timestamp)
		];
		return parts.filter(Boolean).join(' | ');
	}

	function levelLabel(clip: RunClip): string {
		const level = clip.metadata.level || 'unknown';
		return clip.metadata.levelNumber ? `${clip.metadata.levelNumber}. ${level}` : level;
	}

	function romLanguageLabel(lang: string): string | null {
		switch (lang) {
			case 'en':
				return 'English ROM';
			case 'jp':
				return 'Japanese ROM';
			case '':
				return null;
			default:
				return `${lang.toUpperCase()} ROM`;
		}
	}

	function statusLabel(status: string): string {
		switch (status) {
			case 'complete':
				return 'complete';
			case 'failed':
				return 'failed';
			case 'abort':
				return 'aborted';
			case 'kia':
				return 'KIA';
			default:
				return status;
		}
	}

	function formatDate(value: string): string {
		const date = new Date(value);
		return Number.isNaN(date.getTime()) ? value : date.toLocaleString();
	}

	function formatDuration(seconds?: number | null): string | null {
		if (seconds === null || seconds === undefined || !Number.isFinite(seconds)) return null;
		const rounded = Math.max(0, Math.round(seconds));
		const minutes = Math.floor(rounded / 60);
		const secs = rounded % 60;
		return `${minutes}:${secs.toString().padStart(2, '0')}`;
	}

	function formatBytes(bytes: number): string {
		if (!Number.isFinite(bytes) || bytes <= 0) return '0 B';
		const units = ['B', 'KB', 'MB', 'GB'];
		let value = bytes;
		let unit = 0;
		while (value >= 1024 && unit < units.length - 1) {
			value /= 1024;
			unit++;
		}
		return `${value.toFixed(unit === 0 ? 0 : 1)} ${units[unit]}`;
	}

	function metadataRows(clip: RunClip): { label: string; value: string | null | undefined }[] {
		return [
			{ label: 'Level', value: levelLabel(clip) },
			{ label: 'ROM language', value: romLanguageLabel(clip.metadata.romLanguage) },
			{ label: 'Time', value: clip.metadata.time },
			{ label: 'Difficulty', value: clip.metadata.difficulty },
			{ label: 'Status', value: statusLabel(clip.metadata.status) },
			{ label: 'Run timestamp', value: formatDate(clip.metadata.timestamp) },
			{ label: 'Duration', value: formatDuration(clip.durationSecs) },
			{ label: 'Size', value: formatBytes(clip.sizeBytes) },
			{ label: 'Directory', value: clip.directory },
			{ label: 'Created by', value: clip.metadata.comment }
		];
	}
</script>

<svelte:head>
	<title>Runs</title>
</svelte:head>

<svelte:window {onkeydown} />

{#snippet leading(option: Option)}
	{@const clip = clipByPath.get(option.key ?? option.title)}
	{#if clip}
		<img
			src="{runThumbnailUrl(clip.path)}&v={previewVersion}"
			alt="Thumbnail for {clip.fileName}"
			loading="lazy"
			onerror={(e) => ((e.currentTarget as HTMLImageElement).style.visibility = 'hidden')}
			class="obs-preview aspect-video max-h-32 w-full shrink-0 object-contain sm:h-24 sm:w-auto"
		/>
	{/if}
{/snippet}

<main class="mx-auto w-full max-w-3xl px-4 py-8 sm:px-6 sm:py-12">
	<h1 class="obs-heading text-2xl font-semibold">Runs</h1>
	<p class="obs-subtitle mt-2 mb-8 text-sm">Tagged clips from configured output folders.</p>

	{#if error}
		<div class="obs-alert-error mb-4 rounded px-4 py-3">
			<p class="obs-alert-error-title text-sm font-semibold">Could not load runs</p>
			<p class="obs-alert-error-body mt-1 font-mono text-xs">{error}</p>
		</div>
	{/if}

	{#if directoryErrors.length > 0}
		<div class="obs-alert-warning mb-4 rounded px-4 py-3">
			<p class="obs-alert-warning-title text-sm font-semibold">Some folders could not be scanned</p>
			<ul class="obs-alert-warning-body mt-2 space-y-1 font-mono text-xs">
				{#each directoryErrors as dir}
					<li>{dir.kind}: {dir.path} ({dir.error})</li>
				{/each}
			</ul>
		</div>
	{/if}

	{#if loading && runs === null}
		<p class="obs-dim font-mono text-sm">Loading runs...</p>
	{:else if runs && runs.directories.length === 0}
		<div class="obs-empty-state rounded px-4 py-6 text-center">
			<p class="obs-muted text-sm">No run folders configured.</p>
			<p class="obs-dim mt-1 font-mono text-xs">Set completed and failed output folders in Options.</p>
		</div>
	{:else if clips.length === 0}
		<div class="obs-empty-state rounded px-4 py-6 text-center">
			<p class="obs-muted text-sm">No tagged clips found.</p>
			<p class="obs-dim mt-1 font-mono text-xs">New clips saved by this plugin will appear here.</p>
		</div>
	{:else}
		<OptionList {options} onSelect={select} {leading} />
	{/if}

	<div class="mt-6 flex justify-center">
		<button
			type="button"
			onclick={reload}
			disabled={loading}
			class="obs-text-button px-2 py-1 font-mono text-xs underline-offset-2"
		>
			{loading ? 'refreshing...' : 'refresh runs'}
		</button>
	</div>
</main>

{#if selected}
	<div class="obs-overlay fixed inset-0 z-50 flex items-center justify-center p-4">
		<button type="button" aria-label="Close run viewer" class="absolute inset-0 cursor-default" onclick={close}
		></button>
		<dialog
			open
			aria-label="Run video"
			class="obs-dialog relative z-10 m-0 max-h-full w-full max-w-5xl overflow-hidden rounded p-0"
		>
			<header class="obs-dialog-header flex items-start gap-4 px-4 py-3">
				<div class="min-w-0">
					<h2 class="obs-heading truncate text-lg font-semibold">{selected.fileName}</h2>
					<p class="obs-dim mt-1 truncate font-mono text-xs">{runDetail(selected)}</p>
				</div>
				<button type="button" onclick={close} class="obs-text-button ml-auto px-2 py-1 font-mono text-xs">
					close
				</button>
			</header>

			<div class="max-h-[calc(100vh-9rem)] overflow-y-auto p-4">
				<!-- svelte-ignore a11y_media_has_caption -->
				<video src={runVideoUrl(selected.path)} controls class="obs-preview aspect-video w-full"></video>

				<dl class="mt-4 grid grid-cols-1 gap-x-4 gap-y-2 text-sm sm:grid-cols-[9rem_minmax(0,1fr)]">
					{#each metadataRows(selected).filter((row) => row.value) as row}
						<dt class="obs-dim font-mono text-xs">{row.label}</dt>
						<dd class="obs-muted min-w-0 break-words">{row.value}</dd>
					{/each}
				</dl>
			</div>
		</dialog>
	</div>
{/if}
