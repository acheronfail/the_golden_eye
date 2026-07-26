<script lang="ts">
	import { goto } from '$app/navigation';
	import { page } from '$app/state';
	import { backend } from '$lib/api';
	import ActionMenu from '$lib/ui/ActionMenu.svelte';
	import ReadClipsDialog from '$lib/features/runs/ReadClipsDialog.svelte';
	import RunDeleteDialog from '$lib/features/runs/RunDeleteDialog.svelte';
	import RunDetailDialog from '$lib/features/runs/RunDetailDialog.svelte';
	import RunFiltersForm from '$lib/features/runs/RunFilters.svelte';
	import RunImportDialog from '$lib/features/runs/RunImportDialog.svelte';
	import RunList from '$lib/features/runs/RunList.svelte';
	import { RunsPageController } from '$lib/features/runs/runsPageController.svelte';
	import { LEVEL_OPTIONS } from '$lib/features/runs/runsView';
	import { onDestroy, onMount, untrack } from 'svelte';

	const levelSelectOptions = LEVEL_OPTIONS.map((level) => ({ value: level, label: level }));
	const controller = new RunsPageController(
		backend,
		{
			currentUrl: () => new URL(page.url),
			goto: (href, options) => void goto(href, options),
			promptForFilename: (message, initialValue) => prompt(message, initialValue)
		},
		page.url.searchParams.get('sort')
	);

	onMount(() => controller.initialize(navigator.platform));
	onDestroy(() => controller.destroy());

	$effect(() => {
		const requestedRunId = page.url.searchParams.get('runId');
		controller.clips;
		untrack(() => controller.reconcileRequestedRun(requestedRunId));
	});

	const onkeydown = (event: KeyboardEvent) => controller.handleKeydown(event);
</script>

<svelte:head>
	<title>Runs</title>
</svelte:head>

<svelte:window {onkeydown} />

<main class="mx-auto w-full max-w-3xl px-4 obs-page-top pb-4 sm:px-6 sm:pb-6">
	<div class="mb-4 flex items-center gap-3">
		<div class="min-w-0">
			<h1 class="text-2xl font-semibold obs-heading">Runs</h1>
		</div>
		<div class="relative z-30 ml-auto flex">
			<button
				type="button"
				onclick={() => controller.openImport()}
				class="obs-button h-8 rounded-r-none border-r-0 obs-button-gold px-3 font-mono text-xs"
			>
				+ add times
			</button>
			<ActionMenu
				items={controller.runActions}
				label="More run actions"
				title="More run actions"
				busy={controller.folderRevealBusy}
				triggerClass="h-8 w-8 shrink-0 rounded-l-none px-2 font-mono text-sm"
				triggerGlyph="▾"
			/>
		</div>
	</div>

	<RunFiltersForm
		bind:collapsed={controller.filtersCollapsed}
		bind:filters={controller.filters}
		activeFilters={controller.activeFilters}
		hasActiveFilters={controller.hasActiveFilters}
		levelOptions={levelSelectOptions}
		clearFilter={(key) => controller.clearFilter(key)}
		clearFilters={() => controller.clearFilters()}
	/>

	{#if controller.error}
		<div class="mb-4 rounded obs-alert-error px-4 py-3">
			<p class="text-sm font-semibold obs-alert-error-title">Could not load runs</p>
			<p class="mt-1 font-mono text-xs obs-alert-error-body">{controller.error}</p>
		</div>
	{/if}

	{#if controller.directoryErrors.length > 0}
		<div class="mb-4 rounded obs-alert-warning px-4 py-3">
			<p class="text-sm font-semibold obs-alert-warning-title">Some folders could not be scanned</p>
			<ul class="mt-2 space-y-1 font-mono text-xs obs-alert-warning-body">
				{#each controller.directoryErrors as directory}
					<li>{directory.kind}: {directory.path} ({directory.error})</li>
				{/each}
			</ul>
		</div>
	{/if}

	{#if controller.listActionError}
		<div class="mb-4 rounded obs-alert-error px-4 py-3">
			<p class="text-sm font-semibold obs-alert-error-title">Run action failed</p>
			<p class="mt-1 font-mono text-xs obs-alert-error-body">{controller.listActionError}</p>
		</div>
	{/if}

	<RunList
		loading={controller.loading}
		clips={controller.clips}
		visibleClips={controller.visibleClips}
		scannedDirectoryCount={controller.scannedDirectoryCount}
		directoryCount={controller.runs?.directories.length ?? null}
		hasActiveFilters={controller.hasActiveFilters}
		clearFilters={() => controller.clearFilters()}
		sort={controller.sort}
		onSortChange={(sort) => controller.changeSort(sort)}
		fileBrowserLabel={controller.browserLabel}
		busyPath={controller.listActionBusyId}
		open={(clip) => controller.select(clip)}
		rename={(clip) => controller.renameFromList(clip)}
		reveal={(clip) => controller.revealFromList(clip)}
		remove={(clip) => controller.requestDelete(clip)}
		keep={(clip) => controller.keepFromList(clip)}
	/>
</main>

<RunDetailDialog
	clip={controller.selected}
	bind:metadataDraft={controller.metadataDraft}
	view={controller.detailView}
/>
<RunImportDialog
	open={controller.importOpen}
	busy={controller.importBusy}
	error={controller.importError}
	result={controller.importResult}
	onClose={() => controller.importBusy === null && (controller.importOpen = false)}
	onManual={(input) => controller.createManualRun(input)}
	onElite={(username) => controller.importTheElite(username)}
/>
{#if controller.readClipsOpen}
	<ReadClipsDialog cancel={() => (controller.readClipsOpen = false)} read={() => controller.confirmReadClips()} />
{/if}
<RunDeleteDialog
	run={controller.deleteTarget}
	busy={controller.deleteBusy}
	error={controller.deleteError}
	onCancel={() => controller.cancelDelete()}
	onDeleteVideo={() => void controller.confirmDelete(true)}
	onDeleteAll={() => void controller.confirmDelete(false)}
/>
