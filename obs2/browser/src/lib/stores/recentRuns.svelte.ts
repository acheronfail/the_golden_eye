import { backend, type RecordingSavePending, type ReplaySaveStatus, type RunClip } from '$lib/api';
import type { RecentRun } from '$lib/features/monitor/monitorView';
import { MAX_RECENT_RUN_LIMIT } from '$lib/stores/settings.svelte';

const formatRunTime = (seconds: number | undefined): string | undefined => {
	if (seconds === undefined) return undefined;
	const minutes = Math.floor(seconds / 60);
	return `${minutes.toString().padStart(2, '0')}:${(seconds % 60).toString().padStart(2, '0')}`;
};

export class RecentRunsStore {
	items = $state<RecentRun[]>([]);
	loading = $state(false);
	error = $state<string | null>(null);
	busyRunId = $state<string | null>(null);
	private refreshVersion = 0;
	private catalogItems: RunClip[] = [];
	private pendingItems = new Map<number, RecentRun>();
	private activeSaves = new Map<number, string | undefined>();
	private latestTrackingId = 0;

	applySavePending(pending: RecordingSavePending): void {
		const trackingId = pending.trackingId;
		if (this.activeSaves.get(trackingId) || (trackingId <= this.latestTrackingId && !this.activeSaves.has(trackingId)))
			return;
		const previous = this.pendingItems.get(trackingId);
		const timestamp = previous?.metadata.timestamp ?? new Date().toISOString();
		this.pendingItems.set(trackingId, {
			runId: `pending-${trackingId}`,
			path: '',
			fileName: '',
			directory: '',
			sizeBytes: 0,
			saving: true,
			metadata: {
				timestamp,
				time: formatRunTime(pending.timeSecs),
				timeSeconds: pending.timeSecs,
				level: pending.level,
				levelNumber: pending.levelNumber,
				difficulty: pending.difficulty,
				status: 'pending',
				gameLanguage: '',
				sourceName: '',
				comment: '',
				pluginVersion: ''
			},
			retentionState: 'pending',
			retentionReason: null
		});
		this.rebuildItems();
	}

	applyReplaySaves(saves: ReplaySaveStatus[]): void {
		for (const save of saves) this.latestTrackingId = Math.max(this.latestTrackingId, save.trackingId);
		const active = new Map(
			saves
				.filter((save) => save.stage !== 'completed' && save.stage !== 'failed')
				.map((save) => [save.trackingId, save.runId])
		);
		const changed =
			active.size !== this.activeSaves.size ||
			[...active].some(([id, runId]) => !this.activeSaves.has(id) || this.activeSaves.get(id) !== runId);
		const previousPendingCount = this.pendingItems.size;
		for (const id of this.pendingItems.keys()) {
			if (!active.has(id)) this.pendingItems.delete(id);
		}
		this.activeSaves = active;
		if (changed || previousPendingCount !== this.pendingItems.size) this.rebuildItems();
	}

	resetReplaySaves(): void {
		this.latestTrackingId = 0;
		this.applyReplaySaves([]);
	}

	async refresh(): Promise<void> {
		const version = ++this.refreshVersion;
		this.loading = true;
		this.error = null;
		try {
			const items = await backend.getRecentRuns(MAX_RECENT_RUN_LIMIT);
			if (version === this.refreshVersion) {
				this.catalogItems = items;
				this.rebuildItems();
			}
		} catch (error) {
			if (version === this.refreshVersion) this.error = error instanceof Error ? error.message : String(error);
		} finally {
			if (version === this.refreshVersion) this.loading = false;
		}
	}

	async keep(runId: string): Promise<void> {
		this.busyRunId = runId;
		this.error = null;
		try {
			const updated = await backend.keepRun(runId);
			this.catalogItems = this.catalogItems.map((run) => (run.runId === runId ? updated : run));
			this.rebuildItems();
		} catch (error) {
			this.error = error instanceof Error ? error.message : String(error);
		} finally {
			this.busyRunId = null;
		}
	}

	private rebuildItems(): void {
		const catalogIds = new Set(this.catalogItems.map((run) => run.runId));
		for (const [trackingId, runId] of this.activeSaves) {
			if (runId && catalogIds.has(runId)) this.pendingItems.delete(trackingId);
		}
		const activeRunIds = new Set(this.activeSaves.values());
		const pending = [...this.pendingItems.values()].sort((a, b) =>
			b.metadata.timestamp.localeCompare(a.metadata.timestamp)
		);
		const catalog = this.catalogItems.map((run) =>
			!run.path && activeRunIds.has(run.runId) ? { ...run, saving: true } : run
		);
		this.items = [...pending, ...catalog].slice(0, MAX_RECENT_RUN_LIMIT);
	}
}

export const recentRuns = new RecentRunsStore();
