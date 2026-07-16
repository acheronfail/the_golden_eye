<script lang="ts">
	import {
		deleteRun,
		renameRun,
		revealRunFolder,
		revealRun,
		runVideoUrl,
		streamRuns,
		updateRunMetadata,
		type EditableRunMetadata,
		type RunClip,
		type RunDirectoryScan,
		type RunsResponse
	} from '$lib/api';
	import { Select } from '$lib';
	import { settings } from '$lib/settings.svelte';
	import {
		DIFFICULTY_OPTIONS,
		EMPTY_RUN_FILTERS,
		LANGUAGE_OPTIONS,
		LEVEL_OPTIONS,
		STATUS_OPTIONS,
		hasActiveRunFilters,
		isCompleted,
		levelLabel,
		romLanguageLabel,
		statusLabel,
		visibleRunClips,
		type RunFilters
	} from '$lib/runsView';
	import { onDestroy, onMount } from 'svelte';

	const levelSelectOptions = LEVEL_OPTIONS.map((level) => ({ value: level, label: level }));

	let runs = $state<RunsResponse | null>(null);
	let loading = $state(false);
	let error = $state<string | null>(null);
	let selected = $state<RunClip | null>(null);
	let metadataDraft = $state<EditableRunMetadata | null>(null);
	let modalError = $state<string | null>(null);
	let modalBusy = $state<string | null>(null);
	let fileBrowserLabel = $state('show in file browser');
	let folderBrowserLabel = $state('show clips folder');
	let folderRevealBusy = $state(false);
	let folderChooserOpen = $state(false);
	let search = $state('');
	let levelFilter = $state('');
	let difficultyFilter = $state('');
	let statusFilter = $state('');
	let languageFilter = $state('');
	let minTimeFilter = $state('');
	let maxTimeFilter = $state('');
	let filtersCollapsed = $state(false);
	let reloadAbort: AbortController | null = null;

	const currentFilters = $derived<RunFilters>({
		search,
		level: levelFilter,
		difficulty: difficultyFilter,
		status: statusFilter,
		language: languageFilter,
		minTime: minTimeFilter,
		maxTime: maxTimeFilter
	});
	const clips = $derived(runs?.clips ?? []);
	const clipByPath = $derived(new Map(clips.map((clip) => [clip.path, clip])));
	const visibleClips = $derived(visibleRunClips(clips, currentFilters));
	const directoryErrors = $derived((runs?.directories ?? []).filter((dir) => dir.error));
	const scannedDirectoryCount = $derived(runs?.directories.length ?? 0);
	const completedDirectory = $derived((runs?.directories ?? []).find((dir) => dir.kind === 'completed' && dir.exists));
	const failedDirectory = $derived((runs?.directories ?? []).find((dir) => dir.kind === 'failed' && dir.exists));
	const revealableDirectories = $derived(
		[completedDirectory, failedDirectory].filter((dir): dir is RunDirectoryScan => Boolean(dir))
	);
	const hasActiveFilters = $derived(hasActiveRunFilters(currentFilters));
	const activeFilterLabels = $derived(activeRunFilterLabels(currentFilters));
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
		folderBrowserLabel = platformFolderBrowserLabel();
		reload();
	});

	onDestroy(() => {
		reloadAbort?.abort();
	});

	const select = (clip: RunClip) => {
		selected = clipByPath.get(clip.path) ?? clip;
		metadataDraft = draftFromClip(selected);
		modalError = null;
		modalBusy = null;
	};

	const clearFilters = () => {
		search = EMPTY_RUN_FILTERS.search;
		levelFilter = EMPTY_RUN_FILTERS.level;
		difficultyFilter = EMPTY_RUN_FILTERS.difficulty;
		statusFilter = EMPTY_RUN_FILTERS.status;
		languageFilter = EMPTY_RUN_FILTERS.language;
		minTimeFilter = EMPTY_RUN_FILTERS.minTime;
		maxTimeFilter = EMPTY_RUN_FILTERS.maxTime;
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

	function openFolderChooser() {
		if (revealableDirectories.length === 0) return;
		if (!settings.saveFailedRuns) {
			void revealRunsFolder('completed');
			return;
		}
		folderChooserOpen = true;
	}

	function closeFolderChooser() {
		if (folderRevealBusy) return;
		folderChooserOpen = false;
	}

	async function revealRunsFolder(kind: RunDirectoryScan['kind']) {
		folderRevealBusy = true;
		error = null;
		try {
			await revealRunFolder(kind);
			folderChooserOpen = false;
		} catch (err) {
			error = err instanceof Error ? err.message : String(err);
		} finally {
			folderRevealBusy = false;
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

	function platformFolderBrowserLabel(): string {
		const platform = navigator.platform.toLowerCase();
		if (platform.includes('mac')) return 'show clips in finder';
		if (platform.includes('win')) return 'show clips in explorer';
		return 'show clips folder';
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

	function statusTone(status: string): string {
		switch (status === 'completed' ? 'complete' : status) {
			case 'complete':
				return 'border-[color-mix(in_srgb,var(--obs-success),var(--obs-border)_35%)] bg-[var(--obs-success-surface)] text-[var(--obs-success)]';
			case 'failed':
			case 'abort':
			case 'kia':
				return 'border-[color-mix(in_srgb,var(--obs-danger),var(--obs-border)_35%)] bg-[var(--obs-danger-surface)] text-[var(--obs-danger)]';
			default:
				return 'obs-token';
		}
	}

	function runMetaChips(clip: RunClip): { label: string; class: string }[] {
		const status = statusLabel(clip.metadata.status);
		return [
			{ label: levelLabel(clip), class: 'obs-token' },
			{ label: clip.metadata.time ?? '', class: 'obs-token' },
			{ label: clip.metadata.difficulty ?? '', class: 'obs-token' },
			{ label: romLanguageLabel(clip.metadata.romLanguage) ?? '', class: 'obs-token' },
			{ label: status, class: statusTone(clip.metadata.status) }
		].filter((chip) => chip.label);
	}

	function activeRunFilterLabels(filters: RunFilters): string[] {
		return [
			filters.search.trim() ? `search: ${filters.search.trim()}` : '',
			filters.level ? `level: ${filters.level}` : '',
			filters.difficulty ? `difficulty: ${filters.difficulty}` : '',
			filters.status ? `status: ${statusLabel(filters.status)}` : '',
			filters.language ? `language: ${romLanguageLabel(filters.language) ?? filters.language}` : '',
			filters.minTime ? `min: ${filters.minTime}` : '',
			filters.maxTime ? `max: ${filters.maxTime}` : ''
		].filter((label) => label);
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

	function directoryPath(directory: RunDirectoryScan | undefined): string {
		return directory?.path ?? 'Not configured';
	}
</script>

<svelte:head>
	<title>Runs</title>
</svelte:head>

<svelte:window {onkeydown} />

<main class="mx-auto w-full max-w-3xl px-3 py-4 sm:px-4 sm:py-6">
	<div class="mb-4 flex items-center gap-3">
		<div class="min-w-0">
			<h1 class="obs-heading text-xl font-semibold">Runs</h1>
			<p class="obs-dim mt-1 font-mono text-xs">
				{visibleClips.length} of {clips.length}{loading ? ' | scanning...' : ''}
			</p>
		</div>
		<button
			type="button"
			onclick={openFolderChooser}
			disabled={folderRevealBusy || revealableDirectories.length === 0}
			class="obs-text-button ml-auto shrink-0 px-2 py-1 font-mono text-xs underline-offset-2"
			title={revealableDirectories.length > 0 ? 'Choose a clips folder to open' : 'Set a clips folder in Options first'}
		>
			{folderRevealBusy ? 'opening...' : folderBrowserLabel}
		</button>
		<button
			type="button"
			onclick={reload}
			disabled={loading}
			class="obs-text-button shrink-0 px-2 py-1 font-mono text-xs underline-offset-2"
		>
			{loading ? 'loading...' : 'reload'}
		</button>
	</div>

	<form
		class="obs-panel sticky top-0 z-20 mb-4 grid gap-2 rounded px-3 py-3"
		onsubmit={(event) => event.preventDefault()}
	>
		<div class="flex min-w-0 items-center gap-2">
			<button
				type="button"
				class="obs-text-button flex min-w-0 flex-1 items-center justify-between gap-2 px-2 py-1.5 font-mono text-xs"
				aria-expanded={!filtersCollapsed}
				aria-controls="runs-filter-controls"
				onclick={() => (filtersCollapsed = !filtersCollapsed)}
			>
				<span aria-hidden="true">{filtersCollapsed ? 'show' : 'hide'}</span>
				<span class="min-w-0 truncate">
					filters{activeFilterLabels.length ? ` (${activeFilterLabels.length})` : ''}
				</span>
			</button>

			<button
				type="button"
				class="obs-text-button shrink-0 px-2 py-1.5 font-mono text-xs"
				disabled={!hasActiveFilters}
				onclick={clearFilters}
			>
				clear
			</button>
		</div>

		<p class="obs-dim min-w-0 truncate font-mono text-xs" title={activeFilterLabels.join(' | ')}>
			{activeFilterLabels.length ? activeFilterLabels.join(' | ') : 'all runs'}
		</p>

		{#if !filtersCollapsed}
			<div id="runs-filter-controls" class="grid gap-2">
				<label class="sr-only" for="runs-search">Search runs</label>
				<input
					id="runs-search"
					class="obs-input px-3 py-2 font-mono text-sm"
					type="search"
					placeholder="search runs"
					bind:value={search}
				/>
				<div class="grid grid-cols-2 gap-2">
					<label class="sr-only" for="runs-level">Level</label>
					<Select
						id="runs-level"
						class="w-full text-xs"
						bind:value={levelFilter}
						options={[{ value: '', label: 'all levels' }, ...levelSelectOptions]}
					/>

					<label class="sr-only" for="runs-difficulty">Difficulty</label>
					<Select
						id="runs-difficulty"
						class="w-full text-xs"
						bind:value={difficultyFilter}
						options={[{ value: '', label: 'all difficulties' }, ...DIFFICULTY_OPTIONS]}
					/>

					<label class="sr-only" for="runs-status">Status</label>
					<Select
						id="runs-status"
						class="w-full text-xs"
						bind:value={statusFilter}
						options={[{ value: '', label: 'all statuses' }, ...STATUS_OPTIONS]}
					/>

					<label class="sr-only" for="runs-language">Language</label>
					<Select
						id="runs-language"
						class="w-full text-xs"
						bind:value={languageFilter}
						options={[{ value: '', label: 'all languages' }, ...LANGUAGE_OPTIONS]}
					/>
				</div>
				<div class="grid grid-cols-2 gap-2">
					<label class="sr-only" for="runs-min-time">Minimum time</label>
					<input
						id="runs-min-time"
						class="obs-input px-2 py-2 font-mono text-xs"
						inputmode="numeric"
						placeholder="min time"
						bind:value={minTimeFilter}
					/>

					<label class="sr-only" for="runs-max-time">Maximum time</label>
					<input
						id="runs-max-time"
						class="obs-input px-2 py-2 font-mono text-xs"
						inputmode="numeric"
						placeholder="max time"
						bind:value={maxTimeFilter}
					/>
				</div>
			</div>
		{/if}
	</form>

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
	{:else if visibleClips.length === 0}
		<div class="obs-empty-state rounded px-4 py-6 text-center">
			<p class="obs-muted text-sm">No runs match the current filters.</p>
			<button
				type="button"
				class="obs-text-button mt-3 px-2 py-1 font-mono text-xs"
				disabled={!hasActiveFilters}
				onclick={clearFilters}
			>
				clear filters
			</button>
		</div>
	{:else}
		{#if loading}
			<p class="obs-dim mb-3 font-mono text-xs">Search still running...</p>
		{/if}
		<ul class="flex flex-col gap-1.5">
			{#each visibleClips as clip (clip.path)}
				<li>
					<button
						type="button"
						class="obs-list-button group grid min-h-16 w-full grid-cols-[minmax(0,1fr)_auto] items-center gap-2 rounded px-3 py-2 text-left transition-colors"
						class:obs-list-button-success={isCompleted(clip)}
						onclick={() => select(clip)}
					>
						<span class="flex min-w-0 flex-col gap-1.5">
							<span class="flex min-w-0 flex-wrap gap-1">
								{#each runMetaChips(clip) as chip}
									<span class="{chip.class} rounded border px-1.5 py-0.5 font-mono text-[10px] leading-tight">
										{chip.label}
									</span>
								{/each}
							</span>
							<span class="obs-list-title min-w-0 truncate text-sm font-semibold" title={clip.fileName}>
								{clip.fileName}
							</span>
							<span
								class="obs-list-detail min-w-0 truncate font-mono text-[10px]"
								title={formatDate(clip.metadata.timestamp)}
							>
								{formatDate(clip.metadata.timestamp)}
							</span>
						</span>
						<span
							class="obs-list-arrow shrink-0 font-mono transition-transform group-hover:translate-x-1"
							aria-hidden="true"
						>
							→
						</span>
					</button>
				</li>
			{/each}
		</ul>
	{/if}
</main>

{#if folderChooserOpen}
	<div class="obs-overlay fixed inset-0 z-50 flex items-center justify-center p-4">
		<button
			type="button"
			aria-label="Close clips folder chooser"
			class="absolute inset-0 cursor-default"
			onclick={closeFolderChooser}
		></button>
		<dialog
			open
			aria-label="Choose clips folder"
			class="obs-dialog relative z-10 m-0 w-full max-w-sm overflow-hidden rounded p-0"
		>
			<header class="obs-dialog-header px-4 py-3">
				<h2 class="obs-heading text-lg font-semibold">Open clips folder</h2>
				<p class="obs-dim mt-1 font-mono text-xs">Choose which configured output folder to reveal.</p>
			</header>
			<div class="grid gap-3 p-4">
				<button
					type="button"
					class="obs-list-button grid gap-1 rounded px-3 py-3 text-left"
					disabled={folderRevealBusy || !completedDirectory}
					onclick={() => revealRunsFolder('completed')}
				>
					<span class="obs-list-title text-sm font-semibold">Completed clips</span>
					<span class="obs-list-detail font-mono text-xs wrap-break-word">{directoryPath(completedDirectory)}</span>
				</button>
				<button
					type="button"
					class="obs-list-button grid gap-1 rounded px-3 py-3 text-left"
					disabled={folderRevealBusy || !failedDirectory}
					onclick={() => revealRunsFolder('failed')}
				>
					<span class="obs-list-title text-sm font-semibold">Failed clips</span>
					<span class="obs-list-detail font-mono text-xs wrap-break-word">{directoryPath(failedDirectory)}</span>
				</button>
				<div class="flex justify-end">
					<button
						type="button"
						class="obs-text-button px-2 py-1 font-mono text-xs"
						disabled={folderRevealBusy}
						onclick={closeFolderChooser}
					>
						close
					</button>
				</div>
			</div>
		</dialog>
	</div>
{/if}

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
							<Select
								class="w-full"
								placeholder="select level"
								bind:value={metadataDraft.level}
								options={levelSelectOptions}
							/>
						</label>
						<label class="flex min-w-0 flex-col gap-1">
							<span class="obs-dim font-mono text-xs">ROM language</span>
							<Select
								class="w-full"
								placeholder="select language"
								bind:value={metadataDraft.romLanguage}
								options={LANGUAGE_OPTIONS}
							/>
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
							<Select
								class="w-full"
								placeholder="select difficulty"
								bind:value={metadataDraft.difficulty}
								options={DIFFICULTY_OPTIONS}
							/>
						</label>
						<label class="flex min-w-0 flex-col gap-1">
							<span class="obs-dim font-mono text-xs">Status</span>
							<Select
								class="w-full"
								placeholder="select status"
								bind:value={metadataDraft.status}
								options={STATUS_OPTIONS}
							/>
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
