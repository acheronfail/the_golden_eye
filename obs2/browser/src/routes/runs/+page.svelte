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
			class="aspect-video h-24 shrink-0 border border-slate-600 bg-black object-contain"
		/>
	{/if}
{/snippet}

<main class="mx-auto w-full max-w-3xl px-6 py-12">
	<h1 class="text-2xl font-semibold text-amber-300">Runs</h1>
	<p class="mt-2 mb-8 text-sm text-neutral-400">Tagged clips from configured output folders.</p>

	{#if error}
		<div class="mb-4 rounded-md border border-red-500/60 bg-red-950/40 px-4 py-3">
			<p class="text-sm font-semibold text-red-300">Could not load runs</p>
			<p class="mt-1 font-mono text-xs text-red-200/80">{error}</p>
		</div>
	{/if}

	{#if directoryErrors.length > 0}
		<div class="mb-4 rounded-md border border-amber-500/60 bg-amber-950/40 px-4 py-3">
			<p class="text-sm font-semibold text-amber-300">Some folders could not be scanned</p>
			<ul class="mt-2 space-y-1 font-mono text-xs text-amber-200/80">
				{#each directoryErrors as dir}
					<li>{dir.kind}: {dir.path} ({dir.error})</li>
				{/each}
			</ul>
		</div>
	{/if}

	{#if loading && runs === null}
		<p class="font-mono text-sm text-neutral-500">Loading runs...</p>
	{:else if runs && runs.directories.length === 0}
		<div class="rounded-md border border-neutral-700 bg-neutral-950/60 px-4 py-6 text-center">
			<p class="text-sm text-neutral-300">No run folders configured.</p>
			<p class="mt-1 font-mono text-xs text-neutral-500">Set completed and failed output folders in Options.</p>
		</div>
	{:else if clips.length === 0}
		<div class="rounded-md border border-neutral-700 bg-neutral-950/60 px-4 py-6 text-center">
			<p class="text-sm text-neutral-300">No tagged clips found.</p>
			<p class="mt-1 font-mono text-xs text-neutral-500">New clips saved by this plugin will appear here.</p>
		</div>
	{:else}
		<OptionList {options} onSelect={select} {leading} />
	{/if}

	<div class="mt-6 flex justify-center">
		<button
			type="button"
			onclick={reload}
			disabled={loading}
			class="rounded border border-neutral-800 px-2 py-1 font-mono text-xs text-neutral-500 underline-offset-2 transition-colors hover:cursor-pointer hover:border-amber-300 hover:text-amber-300 disabled:text-neutral-700 disabled:no-underline"
		>
			{loading ? 'refreshing...' : 'refresh runs'}
		</button>
	</div>
</main>

{#if selected}
	<div class="fixed inset-0 z-50 flex items-center justify-center bg-black/80 p-4">
		<button type="button" aria-label="Close run viewer" class="absolute inset-0 cursor-default" onclick={close}
		></button>
		<dialog
			open
			aria-label="Run video"
			class="relative z-10 m-0 max-h-full w-full max-w-5xl overflow-hidden rounded-md border border-amber-700 bg-neutral-950 p-0 shadow-2xl"
		>
			<header class="flex items-start gap-4 border-b border-amber-900 px-4 py-3">
				<div class="min-w-0">
					<h2 class="truncate text-lg font-semibold text-amber-300">{selected.fileName}</h2>
					<p class="mt-1 truncate font-mono text-xs text-neutral-500">{runDetail(selected)}</p>
				</div>
				<button
					type="button"
					onclick={close}
					class="ml-auto rounded border border-neutral-700 px-2 py-1 font-mono text-xs text-neutral-400 hover:cursor-pointer hover:border-amber-300 hover:text-amber-300"
				>
					close
				</button>
			</header>

			<div class="max-h-[calc(100vh-9rem)] overflow-y-auto p-4">
				<!-- svelte-ignore a11y_media_has_caption -->
				<video src={runVideoUrl(selected.path)} controls class="aspect-video w-full bg-black"></video>

				<dl class="mt-4 grid grid-cols-[9rem_minmax(0,1fr)] gap-x-4 gap-y-2 text-sm">
					{#each metadataRows(selected).filter((row) => row.value) as row}
						<dt class="font-mono text-xs text-neutral-500">{row.label}</dt>
						<dd class="min-w-0 break-words text-neutral-300">{row.value}</dd>
					{/each}
				</dl>
			</div>
		</dialog>
	</div>
{/if}
