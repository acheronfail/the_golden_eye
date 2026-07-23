import { backend, type RecordingSavePending, type RunClip } from '$lib/api';
import { settings } from '$lib/stores/settings.svelte';

const formatRunTime = (seconds: number | undefined): string | undefined => {
	if (seconds === undefined) return undefined;
	const minutes = Math.floor(seconds / 60);
	return `${minutes.toString().padStart(2, '0')}:${(seconds % 60).toString().padStart(2, '0')}`;
};

export class RecentRunsStore {
	items = $state<RunClip[]>([]);
	loading = $state(false);
	error = $state<string | null>(null);
	busyRunId = $state<string | null>(null);
	private refreshVersion = 0;
	private catalogItems: RunClip[] = [];
	private pendingItems = new Map<number, RunClip>();
	private finalizedSaveIds = new Set<number>();

	applySavePending(pending: RecordingSavePending): void {
		if (this.finalizedSaveIds.has(pending.saveId)) return;
		const previous = this.pendingItems.get(pending.saveId);
		const timestamp = previous?.metadata.timestamp ?? new Date().toISOString();
		this.pendingItems.set(pending.saveId, {
			path: '',
			fileName: '',
			directory: '',
			sizeBytes: 0,
			metadata: {
				timestamp,
				time: formatRunTime(pending.timeSecs),
				timeSeconds: pending.timeSecs,
				level: pending.level,
				levelNumber: pending.levelNumber,
				difficulty: pending.difficulty,
				status: 'pending',
				romLanguage: '',
				sourceName: '',
				comment: '',
				pluginVersion: ''
			},
			retentionState: 'pending',
			retentionReason: null
		});
		this.rebuildItems();
	}

	async refresh(finalizedSaveId?: number): Promise<void> {
		if (finalizedSaveId !== undefined) this.finalizedSaveIds.add(finalizedSaveId);
		const version = ++this.refreshVersion;
		this.loading = true;
		this.error = null;
		try {
			const items = await backend.getRecentRuns(settings.recentRunLimit);
			if (version === this.refreshVersion) {
				this.catalogItems = items;
				for (const saveId of this.finalizedSaveIds) this.pendingItems.delete(saveId);
				this.finalizedSaveIds.clear();
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
		const pending = [...this.pendingItems.values()].sort((a, b) =>
			b.metadata.timestamp.localeCompare(a.metadata.timestamp)
		);
		this.items = [...pending, ...this.catalogItems].slice(0, settings.recentRunLimit);
	}
}

export const recentRuns = new RecentRunsStore();
