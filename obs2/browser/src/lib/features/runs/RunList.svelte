<script lang="ts">
	import type { RunClip, RunSort } from '$lib/api';
	import RunListItem from '$lib/features/runs/RunListItem.svelte';
	import RunSortMenu from '$lib/features/runs/RunSortMenu.svelte';
	import SectionTitle from '$lib/ui/SectionTitle.svelte';
	import { stickyRunListHeader, type RunListRow } from '$lib/features/runs/runsView';

	let {
		loading,
		loadingMore = false,
		clips,
		rows,
		total = clips.length,
		hasMore = false,
		loadMore = () => {},
		scannedDirectoryCount,
		directoryCount,
		hasActiveFilters,
		clearFilters,
		sort,
		onSortChange,
		busyPath = null,
		fileBrowserLabel,
		open,
		rename,
		reveal,
		remove,
		keep = () => {}
	}: {
		loading: boolean;
		loadingMore?: boolean;
		clips: RunClip[];
		rows: RunListRow[];
		total?: number;
		hasMore?: boolean;
		loadMore?: () => void | Promise<void>;
		scannedDirectoryCount: number;
		directoryCount: number | null;
		hasActiveFilters: boolean;
		clearFilters: () => void;
		sort: RunSort;
		onSortChange: (sort: RunSort) => void;
		busyPath?: string | null;
		fileBrowserLabel: string;
		open: (clip: RunClip) => void;
		rename: (clip: RunClip) => void | Promise<void>;
		reveal: (clip: RunClip) => void | Promise<void>;
		remove: (clip: RunClip) => void | Promise<void>;
		keep?: (clip: RunClip) => void | Promise<void>;
	} = $props();

	let openMenuPath = $state<string | null>(null);
	let viewportStart = $state(0);
	let viewportHeight = $state(1000);
	const showDate = $derived(sort === 'fastest' || sort === 'slowest');
	const listHeight = $derived(rows.length === 0 ? 0 : rows[rows.length - 1].top + rows[rows.length - 1].height);
	const pinnedHeader = $derived(stickyRunListHeader(rows, viewportStart));
	const virtualRows = $derived.by(() => {
		const minimum = viewportStart - 400;
		const maximum = viewportStart + viewportHeight + 400;
		let low = 0;
		let high = rows.length;
		while (low < high) {
			const middle = (low + high) >> 1;
			if (rows[middle].top + rows[middle].height < minimum) low = middle + 1;
			else high = middle;
		}
		const start = low;
		while (low < rows.length && rows[low].top <= maximum) low += 1;
		return rows.slice(start, low);
	});

	function setMenuOpen(path: string, open: boolean) {
		if (open) {
			openMenuPath = path;
		} else if (openMenuPath === path) {
			openMenuPath = null;
		}
	}

	function trackViewport(node: HTMLDivElement) {
		let frame = 0;
		const scroller = node.closest<HTMLElement>('.obs-content-scroller');
		const filter = node.closest('main')?.querySelector('form');
		const scrollTarget: HTMLElement | Window = scroller ?? window;
		const update = () => {
			cancelAnimationFrame(frame);
			frame = requestAnimationFrame(() => {
				const rect = node.getBoundingClientRect();
				const viewport = scroller?.getBoundingClientRect();
				const stickyInset =
					Number.parseFloat(getComputedStyle(node).getPropertyValue('--runs-filter-sticky-height')) || 0;
				const viewportTop = (viewport?.top ?? 0) + stickyInset;
				const viewportBottom = viewport?.bottom ?? window.innerHeight;
				viewportStart = Math.max(0, viewportTop - rect.top);
				viewportHeight = Math.max(0, viewportBottom - viewportTop);
			});
		};
		const resizeObserver = typeof ResizeObserver === 'undefined' ? null : new ResizeObserver(update);
		if (filter) resizeObserver?.observe(filter);
		scrollTarget.addEventListener('scroll', update, { passive: true });
		window.addEventListener('resize', update);
		update();
		return {
			destroy() {
				cancelAnimationFrame(frame);
				resizeObserver?.disconnect();
				scrollTarget.removeEventListener('scroll', update);
				window.removeEventListener('resize', update);
			}
		};
	}

	function observeMore(node: HTMLElement) {
		if (typeof IntersectionObserver === 'undefined') return {};
		const root = node.closest<HTMLElement>('.obs-content-scroller');
		const observer = new IntersectionObserver(
			(entries) => {
				if (entries.some((entry) => entry.isIntersecting)) void loadMore();
			},
			{ root, rootMargin: '800px 0px' }
		);
		observer.observe(node);
		return { destroy: () => observer.disconnect() };
	}
</script>

{#if loading && clips.length === 0}
	<p class="font-mono text-sm obs-dim">
		{scannedDirectoryCount === 0 ? 'Searching run folders...' : 'Probing clips...'}
	</p>
{:else if directoryCount === 0}
	<div class="rounded obs-empty-state px-4 py-6 text-center">
		<p class="text-sm obs-muted">No run folders configured.</p>
		<p class="mt-1 font-mono text-xs obs-dim">Set completed and failed output folders in Options.</p>
	</div>
{:else if clips.length === 0 && !hasActiveFilters}
	<div class="rounded obs-empty-state px-4 py-6 text-center">
		<p class="text-sm obs-muted">No tagged clips found.</p>
		<p class="mt-1 font-mono text-xs obs-dim">New clips saved by this plugin will appear here.</p>
	</div>
{:else if clips.length === 0}
	<div class="rounded obs-empty-state px-4 py-6 text-center">
		<p class="text-sm obs-muted">No runs match the current filters.</p>
		<button
			type="button"
			class="mt-3 obs-text-button px-2 py-1 font-mono text-xs"
			disabled={!hasActiveFilters}
			onclick={clearFilters}
		>
			Clear filters
		</button>
	</div>
{:else}
	{#if loading}
		<p class="mb-3 font-mono text-xs obs-dim">Search still running...</p>
	{/if}
	<div class="flex items-center justify-between border-b-2 border-(--obs-border-muted) pb-1">
		<p class="font-mono text-xs">
			<strong>{total}</strong>
			{total === 1 ? 'run' : 'runs'}
			{#if clips.length < total}<span class="obs-dim"> · {clips.length} loaded</span>{/if}
		</p>
		<RunSortMenu {sort} onChange={onSortChange} />
	</div>

	<div use:trackViewport role="list" aria-label="Runs" class="relative" style:--list-height={`${listHeight}px`}>
		<div class="h-[var(--list-height)]" aria-hidden="true"></div>
		{#each virtualRows as row (row.key)}
			<div class="absolute inset-x-0 top-0 translate-y-(--row-y)" style:--row-y={`${row.top}px`}>
				{#if row.type === 'header'}
					<SectionTitle
						title={row.label}
						detail={`${row.count} ${row.count === 1 ? 'run' : 'runs'}`}
						class="h-[38px] bg-(--obs-bg) pt-2"
					/>
				{:else}
					<div role="listitem" class="h-14">
						<RunListItem
							clip={row.clip}
							{showDate}
							busy={busyPath === (row.clip.runId ?? row.clip.path)}
							menuOpen={openMenuPath === (row.clip.runId ?? row.clip.path)}
							onMenuOpenChange={(isOpen) => setMenuOpen(row.clip.runId ?? row.clip.path, isOpen)}
							{fileBrowserLabel}
							{open}
							{rename}
							{reveal}
							{remove}
							{keep}
						/>
					</div>
				{/if}
			</div>
		{/each}
		{#if pinnedHeader}
			<div
				data-sticky-run-header
				aria-hidden="true"
				class="pointer-events-none absolute inset-x-0 top-0 z-10 translate-y-(--row-y)"
				style:--row-y={`${pinnedHeader.top}px`}
			>
				<SectionTitle
					title={pinnedHeader.row.label}
					detail={`${pinnedHeader.row.count} ${pinnedHeader.row.count === 1 ? 'run' : 'runs'}`}
					class="h-[38px] bg-(--obs-bg) pt-2"
				/>
			</div>
		{/if}
	</div>
	{#if hasMore}
		<div use:observeMore class="flex h-12 items-center justify-center font-mono text-xs obs-dim">
			{loadingMore ? 'Loading more runs...' : 'Scroll for more runs'}
		</div>
	{/if}
{/if}
