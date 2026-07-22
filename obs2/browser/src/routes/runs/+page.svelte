<script lang="ts">
	import { backend, type EditableRunMetadata, type RunClip, type RunDirectoryScan, type RunsResponse } from '$lib/api';
	import RunDetailDialog from '$lib/components/RunDetailDialog.svelte';
	import RunFiltersForm from '$lib/components/RunFilters.svelte';
	import RunList from '$lib/components/RunList.svelte';
	import RunsFolderChooser from '$lib/components/RunsFolderChooser.svelte';
	import { settings } from '$lib/stores/settings.svelte';
	import {
		DIFFICULTY_OPTIONS,
		EMPTY_RUN_FILTERS,
		LANGUAGE_OPTIONS,
		LEVEL_OPTIONS,
		STATUS_OPTIONS,
		hasActiveRunFilters,
		activeRunFilterLabels,
		visibleRunClips,
		type RunDetailView,
		type RunFilters
	} from '$lib/utils/runsView';
	import { onDestroy, onMount } from 'svelte';

	const levelSelectOptions = LEVEL_OPTIONS.map((level) => ({ value: level, label: level }));

	let runs = $state<RunsResponse | null>(null);
	let loading = $state(false);
	let error = $state<string | null>(null);
	let selected = $state<RunClip | null>(null);
	let metadataDraft = $state<EditableRunMetadata | null>(null);
	let modalError = $state<string | null>(null);
	let modalBusy = $state<string | null>(null);
	let listActionError = $state<string | null>(null);
	let listActionBusyPath = $state<string | null>(null);
	let fileBrowserLabel = $state('Show in file browser');
	let folderBrowserLabel = $state('show clips folder');
	let folderRevealBusy = $state(false);
	let folderChooserOpen = $state(false);
	let filters = $state<RunFilters>({ ...EMPTY_RUN_FILTERS });
	let filtersCollapsed = $state(false);
	let reloadAbort: AbortController | null = null;
	let metadataSaveInFlight = false;
	let metadataSaveQueued = false;

	const clips = $derived(runs?.clips ?? []);
	const clipByPath = $derived(new Map(clips.map((clip) => [clip.path, clip])));
	const visibleClips = $derived(visibleRunClips(clips, filters));
	const directoryErrors = $derived((runs?.directories ?? []).filter((dir) => dir.error));
	const scannedDirectoryCount = $derived(runs?.directories.length ?? 0);
	const completedDirectory = $derived((runs?.directories ?? []).find((dir) => dir.kind === 'completed' && dir.exists));
	const failedDirectory = $derived((runs?.directories ?? []).find((dir) => dir.kind === 'failed' && dir.exists));
	const revealableDirectories = $derived(
		[completedDirectory, failedDirectory].filter((dir): dir is RunDirectoryScan => Boolean(dir))
	);
	const hasActiveFilters = $derived(hasActiveRunFilters(filters));
	const activeFilterLabels = $derived(activeRunFilterLabels(filters));
	let metadataDirty = $derived.by(() => {
		if (!selected || !metadataDraft) return false;
		return !sameMetadataDraft(metadataDraft, draftFromClip(selected));
	});

	const reload = async (refresh = false) => {
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
			await backend.streamRuns(
				(event) => {
					if (event.type === 'directory') {
						upsertDirectory(event.directory);
					} else if (event.type === 'clip') {
						upsertClip(event.clip);
						if (selectedPath && event.clip.path === selectedPath) selectedFound = true;
					}
				},
				abort.signal,
				{ refresh }
			);
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
		Object.assign(filters, EMPTY_RUN_FILTERS);
	};

	const close = () => {
		void saveMetadata();
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

	function updateClipInList(oldPath: string, clip: RunClip) {
		if (runs) {
			runs = {
				...runs,
				clips: runs.clips.map((candidate) => (candidate.path === oldPath ? clip : candidate))
			};
		}
	}

	function replaceClip(oldPath: string, clip: RunClip) {
		updateClipInList(oldPath, clip);
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
		if (metadataSaveInFlight) {
			metadataSaveQueued = true;
			return;
		}

		metadataSaveInFlight = true;
		modalBusy = 'metadata';
		modalError = null;
		try {
			const oldPath = selected.path;
			const draft = { ...metadataDraft };
			const updated = await backend.updateRunMetadata(oldPath, draft);
			const stillSelected = selected?.path === oldPath;
			const pendingDraft = metadataDraft && !sameMetadataDraft(metadataDraft, draft) ? { ...metadataDraft } : null;
			if (stillSelected) {
				replaceClip(oldPath, updated);
				if (pendingDraft) metadataDraft = pendingDraft;
			} else {
				updateClipInList(oldPath, updated);
			}
		} catch (err) {
			modalError = err instanceof Error ? err.message : String(err);
		} finally {
			metadataSaveInFlight = false;
			modalBusy = null;
			if (metadataSaveQueued) {
				metadataSaveQueued = false;
				void saveMetadata();
			}
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
			const updated = await backend.renameRun(oldPath, fileName);
			replaceClip(oldPath, updated);
		} catch (err) {
			modalError = err instanceof Error ? err.message : String(err);
		} finally {
			modalBusy = null;
		}
	}

	async function renameRunFromList(clip: RunClip) {
		const next = prompt('New filename (extension preserved if omitted):', clip.fileName);
		if (next === null) return;
		const fileName = next.trim();
		if (!fileName || fileName === clip.fileName) return;

		listActionBusyPath = clip.path;
		listActionError = null;
		try {
			const updated = await backend.renameRun(clip.path, fileName);
			updateClipInList(clip.path, updated);
			if (selected?.path === clip.path) replaceClip(clip.path, updated);
		} catch (err) {
			listActionError = err instanceof Error ? err.message : String(err);
		} finally {
			listActionBusyPath = null;
		}
	}

	async function revealSelectedRun() {
		if (!selected) return;
		modalBusy = 'reveal';
		modalError = null;
		try {
			await backend.revealRun(selected.path);
		} catch (err) {
			modalError = err instanceof Error ? err.message : String(err);
		} finally {
			modalBusy = null;
		}
	}

	async function revealRunFromList(clip: RunClip) {
		listActionBusyPath = clip.path;
		listActionError = null;
		try {
			await backend.revealRun(clip.path);
		} catch (err) {
			listActionError = err instanceof Error ? err.message : String(err);
		} finally {
			listActionBusyPath = null;
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
			await backend.revealRunFolder(kind);
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
			await backend.deleteRun(oldPath);
			removeClip(oldPath);
			close();
		} catch (err) {
			modalError = err instanceof Error ? err.message : String(err);
		} finally {
			modalBusy = null;
		}
	}

	async function deleteRunFromList(clip: RunClip) {
		const confirmed = confirm(`Delete "${clip.fileName}" from disk?\n\nThis cannot be undone.`);
		if (!confirmed) return;

		listActionBusyPath = clip.path;
		listActionError = null;
		try {
			await backend.deleteRun(clip.path);
			removeClip(clip.path);
			if (selected?.path === clip.path) {
				selected = null;
				metadataDraft = null;
			}
		} catch (err) {
			listActionError = err instanceof Error ? err.message : String(err);
		} finally {
			listActionBusyPath = null;
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
		if (platform.includes('mac')) return 'Show in Finder';
		if (platform.includes('win')) return 'Show in Explorer';
		return 'Show in file browser';
	}

	function platformFolderBrowserLabel(): string {
		const platform = navigator.platform.toLowerCase();
		if (platform.includes('mac')) return 'show clips in finder';
		if (platform.includes('win')) return 'show clips in explorer';
		return 'show clips folder';
	}

	let detailView = $derived<RunDetailView>({
		modal: {
			error: modalError,
			busy: modalBusy
		},
		display: {
			fileBrowserLabel,
			levelOptions: levelSelectOptions
		},
		actions: {
			close,
			delete: deleteSelectedRun,
			reveal: revealSelectedRun,
			rename: renameSelectedRun,
			saveMetadata,
			normalizeDraftTime
		}
	});
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
			onclick={() => reload(true)}
			disabled={loading}
			class="obs-text-button shrink-0 px-2 py-1 font-mono text-xs underline-offset-2"
		>
			{loading ? 'loading...' : 'reload'}
		</button>
	</div>

	<RunFiltersForm
		bind:collapsed={filtersCollapsed}
		{filters}
		{activeFilterLabels}
		{hasActiveFilters}
		levelOptions={levelSelectOptions}
		{clearFilters}
	/>

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

	{#if listActionError}
		<div class="obs-alert-error mb-4 rounded px-4 py-3">
			<p class="obs-alert-error-title text-sm font-semibold">Run action failed</p>
			<p class="obs-alert-error-body mt-1 font-mono text-xs">{listActionError}</p>
		</div>
	{/if}

	<RunList
		{loading}
		{clips}
		{visibleClips}
		{scannedDirectoryCount}
		directoryCount={runs?.directories.length ?? null}
		{hasActiveFilters}
		{clearFilters}
		{fileBrowserLabel}
		busyPath={listActionBusyPath}
		open={select}
		rename={renameRunFromList}
		reveal={revealRunFromList}
		remove={deleteRunFromList}
	/>
</main>

<RunsFolderChooser
	open={folderChooserOpen}
	busy={folderRevealBusy}
	{completedDirectory}
	{failedDirectory}
	close={closeFolderChooser}
	reveal={revealRunsFolder}
/>

<RunDetailDialog clip={selected} bind:metadataDraft view={detailView} />
