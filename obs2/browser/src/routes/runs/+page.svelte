<script lang="ts">
	import { goto } from '$app/navigation';
	import { page } from '$app/state';
	import {
		backend,
		type EditableRunMetadata,
		type RunClip,
		type RunDirectoryScan,
		type RunsResponse,
		type RunSort
	} from '$lib/api';
	import RunDetailDialog from '$lib/components/RunDetailDialog.svelte';
	import RunDeleteDialog from '$lib/components/RunDeleteDialog.svelte';
	import RunFiltersForm from '$lib/components/RunFilters.svelte';
	import RunList from '$lib/components/RunList.svelte';
	import { settings } from '$lib/stores/settings.svelte';
	import {
		DIFFICULTY_OPTIONS,
		EMPTY_RUN_FILTERS,
		LANGUAGE_OPTIONS,
		LEVEL_OPTIONS,
		STATUS_OPTIONS,
		hasActiveRunFilters,
		activeRunFilters,
		parseRunSort,
		visibleRunClips,
		type RunDetailView,
		type RunFilterKey,
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
	let filters = $state<RunFilters>({ ...EMPTY_RUN_FILTERS });
	let filtersCollapsed = $state(true);
	let sort = $state<RunSort>(parseRunSort(page.url.searchParams.get('sort')));
	let reloadAbort: AbortController | null = null;
	let metadataSavePromise: Promise<boolean> | null = null;
	let deleteTarget = $state<RunClip | null>(null);
	let deleteBusy = $state(false);
	let deleteError = $state<string | null>(null);
	let handledRequestedRunId: string | null = null;

	const runKey = (run: RunClip): string => run.runId;
	const requestedRunId = $derived(page.url.searchParams.get('runId'));

	const clips = $derived(runs?.clips ?? []);
	const clipByPath = $derived(new Map(clips.map((clip) => [runKey(clip), clip])));
	const visibleClips = $derived(visibleRunClips(clips, filters, sort));
	const directoryErrors = $derived((runs?.directories ?? []).filter((dir) => dir.error));
	const scannedDirectoryCount = $derived(runs?.directories.length ?? 0);
	const completedDirectory = $derived((runs?.directories ?? []).find((dir) => dir.kind === 'completed' && dir.exists));
	const revealableDirectories = $derived([completedDirectory].filter((dir): dir is RunDirectoryScan => Boolean(dir)));
	const hasActiveFilters = $derived(hasActiveRunFilters(filters));
	const activeFilters = $derived(activeRunFilters(filters));
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
		const selectedPath = selected ? runKey(selected) : null;
		let selectedFound = false;
		runs = { directories: [], clips: [] };
		try {
			const loaded = await backend.getRuns({ refresh, sort, signal: abort.signal });
			runs = loaded;
			selectedFound = selectedPath ? loaded.clips.some((clip) => runKey(clip) === selectedPath) : false;
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
		selected = clipByPath.get(runKey(clip)) ?? clip;
		metadataDraft = draftFromClip(selected);
		modalError = null;
		modalBusy = null;
	};

	$effect(() => {
		if (!requestedRunId) {
			handledRequestedRunId = null;
			return;
		}
		if (requestedRunId === handledRequestedRunId) return;
		const requested = clips.find((clip) => clip.runId === requestedRunId);
		if (!requested) return;
		handledRequestedRunId = requestedRunId;
		select(requested);
	});

	const clearFilters = () => {
		Object.assign(filters, EMPTY_RUN_FILTERS);
	};

	const clearFilter = (key: RunFilterKey) => {
		filters[key] = '';
	};

	const changeSort = (next: RunSort) => {
		if (sort === next) return;
		sort = next;
		const url = new URL(page.url);
		if (sort === 'newest') url.searchParams.delete('sort');
		else url.searchParams.set('sort', sort);
		void goto(`${url.pathname}${url.search}`, { replaceState: true, noScroll: true, keepFocus: true });
		void reload();
	};

	const close = async () => {
		if (!(await saveMetadata())) return;
		selected = null;
		metadataDraft = null;
		modalError = null;
		modalBusy = null;
		if (page.url.searchParams.has('runId')) {
			const url = new URL(page.url);
			url.searchParams.delete('runId');
			void goto(`${url.pathname}${url.search}`, { replaceState: true, noScroll: true, keepFocus: true });
		}
	};

	const onkeydown = (event: KeyboardEvent) => {
		if (selected && event.key === 'Escape') void close();
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
				clips: runs.clips.map((candidate) => (runKey(candidate) === oldPath ? clip : candidate))
			};
		}
	}

	function replaceClip(oldPath: string, clip: RunClip) {
		updateClipInList(oldPath, clip);
		selected = clip;
		metadataDraft = draftFromClip(clip);
	}

	function removeClip(path: string) {
		if (runs) {
			runs = {
				...runs,
				clips: runs.clips.filter((clip) => runKey(clip) !== path)
			};
		}
	}

	async function saveMetadata(): Promise<boolean> {
		if (!selected || !metadataDraft || !metadataDirty) return true;
		if (metadataSavePromise) {
			if (!(await metadataSavePromise)) return false;
			return saveMetadata();
		}

		const runId = selected.runId;
		const oldKey = runKey(selected);
		const draft = { ...metadataDraft };
		modalBusy = 'metadata';
		modalError = null;
		const request = (async () => {
			try {
				const updated = await backend.updateRunMetadata(runId, draft);
				const stillSelected = selected?.runId === runId;
				const pendingDraft = metadataDraft && !sameMetadataDraft(metadataDraft, draft) ? { ...metadataDraft } : null;
				if (stillSelected) {
					replaceClip(oldKey, updated);
					if (pendingDraft) metadataDraft = pendingDraft;
				} else {
					updateClipInList(oldKey, updated);
				}
				return true;
			} catch (err) {
				modalError = err instanceof Error ? err.message : String(err);
				return false;
			}
		})();
		metadataSavePromise = request;
		const saved = await request;
		if (metadataSavePromise === request) {
			metadataSavePromise = null;
			modalBusy = null;
		}
		if (!saved) return false;
		return saveMetadata();
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
			const oldKey = runKey(selected);
			const updated = await backend.renameRun(selected.path, fileName);
			replaceClip(oldKey, updated);
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
			const key = runKey(clip);
			const updated = await backend.renameRun(clip.path, fileName);
			updateClipInList(key, updated);
			if (selected?.runId === clip.runId) replaceClip(key, updated);
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
		void revealRunsFolder('completed');
	}

	async function revealRunsFolder(kind: RunDirectoryScan['kind']) {
		folderRevealBusy = true;
		error = null;
		try {
			await backend.revealRunFolder(kind);
		} catch (err) {
			error = err instanceof Error ? err.message : String(err);
		} finally {
			folderRevealBusy = false;
		}
	}

	async function deleteSelectedRun() {
		if (!selected) return;
		deleteTarget = selected;
		deleteError = null;
	}

	async function deleteRunFromList(clip: RunClip) {
		deleteTarget = clip;
		deleteError = null;
	}

	async function confirmDelete(keepHistory: boolean) {
		if (!deleteTarget) return;
		const target = deleteTarget;
		const runId = target.runId;
		deleteBusy = true;
		deleteError = null;
		try {
			const updated = await backend.deleteCatalogRun(runId, keepHistory);
			const key = runKey(target);
			if (updated) updateClipInList(key, updated);
			else removeClip(key);
			if (selected && runKey(selected) === key) {
				selected = updated;
				metadataDraft = updated ? draftFromClip(updated) : null;
			}
			deleteTarget = null;
		} catch (err) {
			deleteError = err instanceof Error ? err.message : String(err);
		} finally {
			deleteBusy = false;
		}
	}

	async function keepRun(clip: RunClip) {
		listActionBusyPath = runKey(clip);
		listActionError = null;
		try {
			const updated = await backend.keepRun(clip.runId);
			updateClipInList(runKey(clip), updated);
			if (selected && runKey(selected) === runKey(clip)) selected = updated;
		} catch (err) {
			listActionError = err instanceof Error ? err.message : String(err);
		} finally {
			listActionBusyPath = null;
		}
	}

	async function keepSelectedRun() {
		if (!selected) return;
		const runId = selected.runId;
		const target = selected;
		const key = runKey(target);
		modalBusy = 'keep';
		modalError = null;
		try {
			const updated = await backend.keepRun(runId);
			updateClipInList(key, updated);
			if (selected?.runId === target.runId) selected = updated;
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
			keep: keepSelectedRun,
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
		bind:filters
		{activeFilters}
		{hasActiveFilters}
		levelOptions={levelSelectOptions}
		{clearFilter}
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
		{sort}
		onSortChange={changeSort}
		{fileBrowserLabel}
		busyPath={listActionBusyPath}
		open={select}
		rename={renameRunFromList}
		reveal={revealRunFromList}
		remove={deleteRunFromList}
		keep={keepRun}
	/>
</main>

<RunDetailDialog clip={selected} bind:metadataDraft view={detailView} />
<RunDeleteDialog
	run={deleteTarget}
	busy={deleteBusy}
	error={deleteError}
	onCancel={() => !deleteBusy && (deleteTarget = null)}
	onDeleteVideo={() => void confirmDelete(true)}
	onDeleteAll={() => void confirmDelete(false)}
/>
