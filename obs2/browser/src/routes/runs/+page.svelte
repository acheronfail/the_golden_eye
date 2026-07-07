<script lang="ts">
	import {
		deleteRun,
		renameRun,
		revealRun,
		runThumbnailUrl,
		runVideoUrl,
		streamRuns,
		updateRunMetadata,
		type EditableRunMetadata,
		type RunClip,
		type RunDirectoryScan,
		type RunsResponse
	} from '$lib/api';
	import OptionList, { type Option } from '$lib/wizard/OptionList.svelte';
	import { onDestroy, onMount } from 'svelte';

	let runs = $state<RunsResponse | null>(null);
	let loading = $state(false);
	let error = $state<string | null>(null);
	let selected = $state<RunClip | null>(null);
	let metadataDraft = $state<EditableRunMetadata | null>(null);
	let modalError = $state<string | null>(null);
	let modalBusy = $state<string | null>(null);
	let fileBrowserLabel = $state('show in file browser');
	let previewVersion = $state(0);
	let reloadAbort: AbortController | null = null;

	const LANGUAGE_OPTIONS = [
		{ value: 'en', label: 'en' },
		{ value: 'jp', label: 'jp' }
	];
	const STATUS_OPTIONS = [
		{ value: 'failed', label: 'failed' },
		{ value: 'abort', label: 'aborted' },
		{ value: 'complete', label: 'completed' },
		{ value: 'kia', label: 'killed in action' }
	];
	const DIFFICULTY_OPTIONS = [
		{ value: 'Agent', label: 'agent' },
		{ value: 'Secret Agent', label: 'secret agent' },
		{ value: '00 Agent', label: '00 agent' },
		{ value: '007', label: '007' }
	];
	const LEVEL_OPTIONS = [
		'Dam',
		'Facility',
		'Runway',
		'Surface 1',
		'Bunker 1',
		'Silo',
		'Frigate',
		'Surface 2',
		'Bunker 2',
		'Statue',
		'Archives',
		'Streets',
		'Depot',
		'Train',
		'Jungle',
		'Control',
		'Caverns',
		'Cradle',
		'Aztec',
		'Egypt'
	];

	const clips = $derived(runs?.clips ?? []);
	const clipByPath = $derived(new Map(clips.map((clip) => [clip.path, clip])));
	const options = $derived<Option[]>(
		clips.map((clip) => ({
			title: clip.fileName,
			detail: runDetail(clip),
			key: clip.path,
			tone: isCompleted(clip) ? 'success' : undefined
		}))
	);
	const directoryErrors = $derived((runs?.directories ?? []).filter((dir) => dir.error));
	const scannedDirectoryCount = $derived(runs?.directories.length ?? 0);
	let metadataDirty = $derived.by(() => {
		if (!selected || !metadataDraft) return false;
		return !sameMetadataDraft(metadataDraft, draftFromClip(selected));
	});

	const reload = async () => {
		if (loading) return;
		reloadAbort?.abort();
		const abort = new AbortController();
		reloadAbort = abort;
		loading = true;
		error = null;
		const selectedPath = selected?.path;
		let selectedFound = false;
		runs = { directories: [], clips: [] };
		try {
			await streamRuns((event) => {
				if (event.type === 'directory') {
					upsertDirectory(event.directory);
				} else if (event.type === 'clip') {
					upsertClip(event.clip);
					if (selectedPath && event.clip.path === selectedPath) selectedFound = true;
				}
			}, abort.signal);
			if (selectedPath && !selectedFound) {
				selected = null;
				metadataDraft = null;
			}
			previewVersion++;
		} catch (err) {
			if (!abort.signal.aborted) error = err instanceof Error ? err.message : String(err);
		} finally {
			if (reloadAbort === abort) {
				loading = false;
				reloadAbort = null;
			}
		}
	};

	onMount(() => {
		fileBrowserLabel = platformFileBrowserLabel();
		reload();
	});

	onDestroy(() => {
		reloadAbort?.abort();
	});

	const select = (option: Option) => {
		selected = clipByPath.get(option.key ?? option.title) ?? null;
		metadataDraft = selected ? draftFromClip(selected) : null;
		modalError = null;
		modalBusy = null;
	};

	const close = () => {
		selected = null;
		metadataDraft = null;
		modalError = null;
		modalBusy = null;
	};

	const onkeydown = (event: KeyboardEvent) => {
		if (selected && event.key === 'Escape') close();
	};

	function isCompleted(clip: RunClip): boolean {
		return clip.metadata.status === 'complete' || clip.metadata.status === 'completed';
	}

	function hasValue(options: { value: string }[], value: string | undefined | null): value is string {
		return Boolean(value && options.some((option) => option.value === value));
	}

	function hasLevel(value: string | undefined | null): value is string {
		return Boolean(value && LEVEL_OPTIONS.includes(value));
	}

	function draftFromClip(clip: RunClip): EditableRunMetadata {
		return {
			romLanguage: hasValue(LANGUAGE_OPTIONS, clip.metadata.romLanguage) ? clip.metadata.romLanguage : '',
			status: hasValue(STATUS_OPTIONS, clip.metadata.status) ? clip.metadata.status : '',
			difficulty: hasValue(DIFFICULTY_OPTIONS, clip.metadata.difficulty) ? clip.metadata.difficulty : '',
			time: clip.metadata.time ?? '',
			level: hasLevel(clip.metadata.level) ? clip.metadata.level : ''
		};
	}

	function sameMetadataDraft(a: EditableRunMetadata, b: EditableRunMetadata): boolean {
		return (
			a.romLanguage === b.romLanguage &&
			a.status === b.status &&
			a.difficulty === b.difficulty &&
			a.time === b.time &&
			a.level === b.level
		);
	}

	function replaceClip(oldPath: string, clip: RunClip) {
		if (runs) {
			runs = {
				...runs,
				clips: runs.clips.map((candidate) => (candidate.path === oldPath ? clip : candidate))
			};
		}
		selected = clip;
		metadataDraft = draftFromClip(clip);
		previewVersion++;
	}

	function emptyRuns(): RunsResponse {
		return { directories: [], clips: [] };
	}

	function upsertDirectory(directory: RunDirectoryScan) {
		const current = runs ?? emptyRuns();
		const index = current.directories.findIndex(
			(candidate) => candidate.kind === directory.kind && candidate.path === directory.path
		);
		const directories =
			index === -1
				? [...current.directories, directory]
				: current.directories.map((candidate, i) => (i === index ? directory : candidate));
		runs = { ...current, directories };
	}

	function upsertClip(clip: RunClip) {
		const current = runs ?? emptyRuns();
		const index = current.clips.findIndex((candidate) => candidate.path === clip.path);
		const clips =
			index === -1 ? [...current.clips, clip] : current.clips.map((candidate, i) => (i === index ? clip : candidate));
		runs = { ...current, clips };
		if (selected?.path === clip.path) {
			selected = clip;
			metadataDraft = draftFromClip(clip);
		}
	}

	function removeClip(path: string) {
		if (runs) {
			runs = {
				...runs,
				clips: runs.clips.filter((clip) => clip.path !== path)
			};
		}
		previewVersion++;
	}

	async function saveMetadata() {
		if (!selected || !metadataDraft || !metadataDirty) return;
		modalBusy = 'metadata';
		modalError = null;
		try {
			const oldPath = selected.path;
			const updated = await updateRunMetadata(oldPath, metadataDraft);
			replaceClip(oldPath, updated);
		} catch (err) {
			modalError = err instanceof Error ? err.message : String(err);
		} finally {
			modalBusy = null;
		}
	}

	async function renameSelectedRun() {
		if (!selected) return;
		const next = prompt('New filename (extension preserved if omitted):', selected.fileName);
		if (next === null) return;
		const fileName = next.trim();
		if (!fileName || fileName === selected.fileName) return;

		modalBusy = 'rename';
		modalError = null;
		try {
			const oldPath = selected.path;
			const updated = await renameRun(oldPath, fileName);
			replaceClip(oldPath, updated);
		} catch (err) {
			modalError = err instanceof Error ? err.message : String(err);
		} finally {
			modalBusy = null;
		}
	}

	async function revealSelectedRun() {
		if (!selected) return;
		modalBusy = 'reveal';
		modalError = null;
		try {
			await revealRun(selected.path);
		} catch (err) {
			modalError = err instanceof Error ? err.message : String(err);
		} finally {
			modalBusy = null;
		}
	}

	async function deleteSelectedRun() {
		if (!selected) return;
		const confirmed = confirm(`Delete "${selected.fileName}" from disk?\n\nThis cannot be undone.`);
		if (!confirmed) return;

		modalBusy = 'delete';
		modalError = null;
		try {
			const oldPath = selected.path;
			await deleteRun(oldPath);
			removeClip(oldPath);
			close();
		} catch (err) {
			modalError = err instanceof Error ? err.message : String(err);
		} finally {
			modalBusy = null;
		}
	}

	function normalizeDraftTime() {
		if (!metadataDraft) return;
		metadataDraft.time = normalizeTimeInput(metadataDraft.time);
	}

	function normalizeTimeInput(value: string): string {
		const trimmed = value.trim();
		if (!trimmed) return '';
		const [minutes, seconds, extra] = trimmed.split(':');
		if (extra !== undefined || !minutes || seconds === undefined) return trimmed;
		if (!/^\d+$/.test(minutes) || !/^\d{1,2}$/.test(seconds)) return trimmed;
		const minuteValue = Number(minutes);
		const secondValue = Number(seconds);
		if (!Number.isInteger(minuteValue) || !Number.isInteger(secondValue) || secondValue > 59) return trimmed;
		return `${minuteValue.toString().padStart(2, '0')}:${secondValue.toString().padStart(2, '0')}`;
	}

	function platformFileBrowserLabel(): string {
		const platform = navigator.platform.toLowerCase();
		if (platform.includes('mac')) return 'show in finder';
		if (platform.includes('win')) return 'show in explorer';
		return 'show in file browser';
	}

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
				return 'EN';
			case 'jp':
				return 'JP';
			case '':
				return null;
			default:
				return `${lang.toUpperCase()}`;
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

	function fileRows(clip: RunClip): { label: string; value: string | null | undefined }[] {
		return [
			{ label: 'Run timestamp', value: formatDate(clip.metadata.timestamp) },
			{ label: 'Duration', value: formatDuration(clip.durationSecs) },
			{ label: 'Size', value: formatBytes(clip.sizeBytes) },
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
	<div class="mb-8 flex items-start gap-4">
		<div class="min-w-0">
			<h1 class="obs-heading text-2xl font-semibold">Runs</h1>
			<p class="obs-subtitle mt-2 text-sm">
				These are clips that have been created by this plugin and found in the folders configured in Options. If you're
				not seeing clips you expect, make sure the plugin is configured to save them in the correct folders.
			</p>
		</div>
		<button
			type="button"
			onclick={reload}
			disabled={loading}
			class="obs-text-button ml-auto shrink-0 px-2 py-1 font-mono text-xs underline-offset-2"
		>
			{loading ? 'loading...' : 'reload'}
		</button>
	</div>

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

	{#if loading && clips.length === 0}
		<p class="obs-dim font-mono text-sm">
			{scannedDirectoryCount === 0 ? 'Searching run folders...' : 'Probing clips...'}
		</p>
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
		{#if loading}
			<p class="obs-dim mb-3 font-mono text-xs">Search still running...</p>
		{/if}
		<OptionList {options} onSelect={select} {leading} />
	{/if}
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
			<header class="obs-dialog-header px-4 py-3">
				<h2 class="obs-heading truncate text-lg font-semibold" title={selected.fileName}>{selected.fileName}</h2>
				<p class="obs-dim mt-1 truncate font-mono text-xs" title={runDetail(selected)}>{runDetail(selected)}</p>
			</header>

			<div class="max-h-[calc(100vh-9rem)] overflow-y-auto p-4">
				<div class="mb-4 flex flex-wrap justify-end gap-2">
					<button
						type="button"
						onclick={deleteSelectedRun}
						disabled={modalBusy !== null}
						class="obs-text-button obs-button-danger px-2 py-1 font-mono text-xs"
					>
						delete
					</button>
					<button
						type="button"
						onclick={revealSelectedRun}
						disabled={modalBusy !== null}
						class="obs-text-button px-2 py-1 font-mono text-xs"
					>
						{fileBrowserLabel}
					</button>
					<button
						type="button"
						onclick={renameSelectedRun}
						disabled={modalBusy !== null}
						class="obs-text-button px-2 py-1 font-mono text-xs"
					>
						rename
					</button>
					<button type="button" onclick={close} class="obs-text-button px-2 py-1 font-mono text-xs"> close </button>
				</div>
				<!-- svelte-ignore a11y_media_has_caption -->
				<video src={runVideoUrl(selected.path)} controls class="obs-preview aspect-video w-full"></video>

				{#if modalError}
					<div class="obs-alert-error mt-4 rounded px-4 py-3">
						<p class="obs-alert-error-title text-sm font-semibold">Run update failed</p>
						<p class="obs-alert-error-body mt-1 font-mono text-xs">{modalError}</p>
					</div>
				{/if}

				{#if metadataDraft}
					<div class="mt-4 grid grid-cols-1 gap-3 text-sm sm:grid-cols-2">
						<label class="flex min-w-0 flex-col gap-1">
							<span class="obs-dim font-mono text-xs">Level</span>
							<select class="obs-select w-full" bind:value={metadataDraft.level}>
								{#if !metadataDraft.level}
									<option value="" disabled>select level</option>
								{/if}
								{#each LEVEL_OPTIONS as level}
									<option value={level}>{level}</option>
								{/each}
							</select>
						</label>
						<label class="flex min-w-0 flex-col gap-1">
							<span class="obs-dim font-mono text-xs">ROM language</span>
							<select class="obs-select w-full" bind:value={metadataDraft.romLanguage}>
								{#if !metadataDraft.romLanguage}
									<option value="" disabled>select language</option>
								{/if}
								{#each LANGUAGE_OPTIONS as option}
									<option value={option.value}>{option.label}</option>
								{/each}
							</select>
						</label>
						<label class="flex min-w-0 flex-col gap-1">
							<span class="obs-dim font-mono text-xs">Time</span>
							<input
								class="obs-input px-3 py-2 font-mono"
								bind:value={metadataDraft.time}
								onblur={normalizeDraftTime}
								inputmode="numeric"
								pattern="[0-9]+:[0-5][0-9]"
								placeholder="mm:ss"
							/>
						</label>
						<label class="flex min-w-0 flex-col gap-1">
							<span class="obs-dim font-mono text-xs">Difficulty</span>
							<select class="obs-select w-full" bind:value={metadataDraft.difficulty}>
								{#if !metadataDraft.difficulty}
									<option value="" disabled>select difficulty</option>
								{/if}
								{#each DIFFICULTY_OPTIONS as option}
									<option value={option.value}>{option.label}</option>
								{/each}
							</select>
						</label>
						<label class="flex min-w-0 flex-col gap-1">
							<span class="obs-dim font-mono text-xs">Status</span>
							<select class="obs-select w-full" bind:value={metadataDraft.status}>
								{#if !metadataDraft.status}
									<option value="" disabled>select status</option>
								{/if}
								{#each STATUS_OPTIONS as option}
									<option value={option.value}>{option.label}</option>
								{/each}
							</select>
						</label>
					</div>

					<div class="mt-4 flex justify-end">
						<button
							type="button"
							onclick={saveMetadata}
							disabled={modalBusy !== null || !metadataDirty}
							class="obs-button obs-button-gold px-3 py-2 font-mono text-xs"
						>
							{modalBusy === 'metadata' ? 'saving...' : 'save metadata'}
						</button>
					</div>
				{/if}

				<dl class="mt-4 grid grid-cols-1 gap-x-4 gap-y-2 text-sm sm:grid-cols-[9rem_minmax(0,1fr)]">
					{#each fileRows(selected).filter((row) => row.value) as row}
						<dt class="obs-dim font-mono text-xs">{row.label}</dt>
						<dd class="obs-muted min-w-0 wrap-break-word">{row.value}</dd>
					{/each}
				</dl>
			</div>
		</dialog>
	</div>
{/if}
