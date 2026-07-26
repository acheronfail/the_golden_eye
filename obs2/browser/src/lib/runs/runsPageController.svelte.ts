import type {
	Backend,
	EditableRunMetadata,
	ManualRunInput,
	RunClip,
	RunDirectoryScan,
	RunsResponse,
	RunSort,
	TheEliteImportResponse
} from '$lib/api';
import type { ActionMenuItem } from '$lib/components/ActionMenu.svelte';
import type { RunDetailView } from '$lib/utils/runsView';
import {
	activeRunFilters,
	EMPTY_RUN_FILTERS,
	hasActiveRunFilters,
	LEVEL_OPTIONS,
	parseRunSort,
	visibleRunClips,
	type RunFilterKey,
	type RunFilters
} from '$lib/utils/runsView';
import { metadataDraftFromClip, normalizeRunTimeInput, runBrowserLabels, sameMetadataDraft } from './runMetadata';

type RunsBackend = Pick<
	Backend,
	| 'getRuns'
	| 'updateRunMetadata'
	| 'renameRun'
	| 'revealRun'
	| 'revealRunFolder'
	| 'deleteCatalogRun'
	| 'keepRun'
	| 'createManualRun'
	| 'importTheElite'
>;

export interface RunsPageNavigation {
	currentUrl: () => URL;
	goto: (href: string, options?: { replaceState?: boolean; noScroll?: boolean; keepFocus?: boolean }) => void;
	promptForFilename: (message: string, initialValue: string) => string | null;
}

type ActionScope = 'modal' | 'list';

const errorMessage = (error: unknown): string => (error instanceof Error ? error.message : String(error));
const runKey = (run: RunClip): string => run.runId;
const levelSelectOptions = LEVEL_OPTIONS.map((level) => ({ value: level, label: level }));

export class RunsPageController {
	runs = $state<RunsResponse | null>(null);
	loading = $state(false);
	error = $state<string | null>(null);
	selected = $state<RunClip | null>(null);
	metadataDraft = $state<EditableRunMetadata | null>(null);
	modalError = $state<string | null>(null);
	modalBusy = $state<string | null>(null);
	listActionError = $state<string | null>(null);
	listActionBusyId = $state<string | null>(null);
	fileBrowserLabel = $state('Show in file browser');
	folderBrowserLabel = $state('show clips folder');
	folderRevealBusy = $state(false);
	filters = $state<RunFilters>({ ...EMPTY_RUN_FILTERS });
	filtersCollapsed = $state(true);
	sort = $state<RunSort>('newest');
	deleteTarget = $state<RunClip | null>(null);
	deleteBusy = $state(false);
	deleteError = $state<string | null>(null);
	importOpen = $state(false);
	importBusy = $state<'manual' | 'elite' | null>(null);
	importError = $state<string | null>(null);
	importResult = $state<TheEliteImportResponse | null>(null);
	readClipsOpen = $state(false);

	private reloadAbort: AbortController | null = null;
	private metadataSavePromise: Promise<boolean> | null = null;
	private handledRequestedRunId: string | null = null;

	constructor(
		private readonly api: RunsBackend,
		private readonly navigation: RunsPageNavigation,
		initialSort?: string | null
	) {
		this.sort = parseRunSort(initialSort);
	}

	get clips(): RunClip[] {
		return this.runs?.clips ?? [];
	}

	get visibleClips(): RunClip[] {
		return visibleRunClips(this.clips, this.filters, this.sort);
	}

	get directoryErrors(): RunDirectoryScan[] {
		return (this.runs?.directories ?? []).filter((directory) => directory.error);
	}

	get scannedDirectoryCount(): number {
		return this.runs?.directories.length ?? 0;
	}

	get revealableDirectories(): RunDirectoryScan[] {
		const completed = (this.runs?.directories ?? []).find(
			(directory) => directory.kind === 'completed' && directory.exists
		);
		return completed ? [completed] : [];
	}

	get hasActiveFilters(): boolean {
		return hasActiveRunFilters(this.filters);
	}

	get activeFilters() {
		return activeRunFilters(this.filters);
	}

	get metadataDirty(): boolean {
		return Boolean(
			this.selected &&
			this.metadataDraft &&
			!sameMetadataDraft(this.metadataDraft, metadataDraftFromClip(this.selected))
		);
	}

	get runActions(): ActionMenuItem[] {
		return [
			{
				label: this.folderRevealBusy ? 'opening...' : this.folderBrowserLabel,
				action: () => this.openFolder(),
				disabled: this.folderRevealBusy || this.revealableDirectories.length === 0
			},
			{ label: 'read clips', action: () => (this.readClipsOpen = true), disabled: this.loading }
		];
	}

	get detailView(): RunDetailView {
		return {
			modal: { error: this.modalError, busy: this.modalBusy },
			display: { fileBrowserLabel: this.fileBrowserLabel, levelOptions: levelSelectOptions },
			actions: {
				close: () => this.close(),
				delete: () => this.requestDelete(this.selected),
				keep: () => this.keep(this.selected, 'modal'),
				reveal: () => this.reveal(this.selected, 'modal'),
				rename: () => this.rename(this.selected, 'modal'),
				saveMetadata: () => this.saveMetadata(),
				normalizeDraftTime: () => this.normalizeDraftTime()
			}
		};
	}

	initialize(platform: string): void {
		const labels = runBrowserLabels(platform);
		this.fileBrowserLabel = labels.file;
		this.folderBrowserLabel = labels.folder;
		void this.reload();
	}

	destroy(): void {
		this.reloadAbort?.abort();
	}

	async reload(refresh = false): Promise<void> {
		this.reloadAbort?.abort();
		const abort = new AbortController();
		this.reloadAbort = abort;
		this.loading = true;
		this.error = null;
		const selectedId = this.selected?.runId ?? null;
		this.runs = { directories: [], clips: [] };
		try {
			const loaded = await this.api.getRuns({ refresh, sort: this.sort, signal: abort.signal });
			if (abort.signal.aborted) return;
			this.runs = loaded;
			if (selectedId && !loaded.clips.some((clip) => clip.runId === selectedId)) {
				this.selected = null;
				this.metadataDraft = null;
			}
		} catch (error) {
			if (!abort.signal.aborted) this.error = errorMessage(error);
		} finally {
			if (this.reloadAbort === abort) {
				this.loading = false;
				this.reloadAbort = null;
			}
		}
	}

	select(clip: RunClip): void {
		this.selected = this.clips.find((candidate) => candidate.runId === clip.runId) ?? clip;
		this.metadataDraft = metadataDraftFromClip(this.selected);
		this.modalError = null;
		this.modalBusy = null;
	}

	reconcileRequestedRun(runId: string | null): void {
		if (!runId) {
			this.handledRequestedRunId = null;
			return;
		}
		if (runId === this.handledRequestedRunId) return;
		const requested = this.clips.find((clip) => clip.runId === runId);
		if (!requested) return;
		this.handledRequestedRunId = runId;
		this.select(requested);
	}

	clearFilters(): void {
		Object.assign(this.filters, EMPTY_RUN_FILTERS);
	}

	clearFilter(key: RunFilterKey): void {
		this.filters[key] = '';
	}

	changeSort(next: RunSort): void {
		if (this.sort === next) return;
		this.sort = next;
		const url = new URL(this.navigation.currentUrl());
		if (this.sort === 'newest') url.searchParams.delete('sort');
		else url.searchParams.set('sort', this.sort);
		this.navigation.goto(`${url.pathname}${url.search}`, {
			replaceState: true,
			noScroll: true,
			keepFocus: true
		});
		void this.reload();
	}

	async close(): Promise<void> {
		if (!(await this.saveMetadata())) return;
		this.selected = null;
		this.metadataDraft = null;
		this.modalError = null;
		this.modalBusy = null;
		const url = new URL(this.navigation.currentUrl());
		if (url.searchParams.has('runId')) {
			url.searchParams.delete('runId');
			this.navigation.goto(`${url.pathname}${url.search}`, {
				replaceState: true,
				noScroll: true,
				keepFocus: true
			});
		}
	}

	handleKeydown(event: KeyboardEvent): void {
		if (this.selected && event.key === 'Escape') void this.close();
	}

	async saveMetadata(): Promise<boolean> {
		if (!this.selected || !this.metadataDraft || !this.metadataDirty) return true;
		if (this.metadataSavePromise) {
			if (!(await this.metadataSavePromise)) return false;
			return this.saveMetadata();
		}

		const runId = this.selected.runId;
		const draft = { ...this.metadataDraft };
		this.modalBusy = 'metadata';
		this.modalError = null;
		const request = (async () => {
			try {
				const updated = await this.api.updateRunMetadata(runId, draft);
				const stillSelected = this.selected?.runId === runId;
				const pendingDraft =
					this.metadataDraft && !sameMetadataDraft(this.metadataDraft, draft) ? { ...this.metadataDraft } : null;
				this.updateClip(updated);
				if (stillSelected) {
					this.selected = updated;
					this.metadataDraft = pendingDraft ?? metadataDraftFromClip(updated);
				}
				return true;
			} catch (error) {
				this.modalError = errorMessage(error);
				return false;
			}
		})();
		this.metadataSavePromise = request;
		const saved = await request;
		if (this.metadataSavePromise === request) {
			this.metadataSavePromise = null;
			this.modalBusy = null;
		}
		if (!saved) return false;
		return this.saveMetadata();
	}

	renameFromList(clip: RunClip): Promise<void> {
		return this.rename(clip, 'list');
	}

	revealFromList(clip: RunClip): Promise<void> {
		return this.reveal(clip, 'list');
	}

	keepFromList(clip: RunClip): Promise<void> {
		return this.keep(clip, 'list');
	}

	requestDelete(clip: RunClip | null): void {
		if (!clip) return;
		this.deleteTarget = clip;
		this.deleteError = null;
	}

	async confirmDelete(keepHistory: boolean): Promise<void> {
		if (!this.deleteTarget) return;
		const target = this.deleteTarget;
		this.deleteBusy = true;
		this.deleteError = null;
		try {
			const updated = await this.api.deleteCatalogRun(target.runId, keepHistory);
			if (updated) this.updateClip(updated);
			else this.removeClip(target.runId);
			if (this.selected?.runId === target.runId) {
				this.selected = updated;
				this.metadataDraft = updated ? metadataDraftFromClip(updated) : null;
			}
			this.deleteTarget = null;
		} catch (error) {
			this.deleteError = errorMessage(error);
		} finally {
			this.deleteBusy = false;
		}
	}

	openImport(): void {
		this.importOpen = true;
		this.importError = null;
		this.importResult = null;
	}

	async createManualRun(input: ManualRunInput): Promise<void> {
		this.importBusy = 'manual';
		this.importError = null;
		try {
			const created = await this.api.createManualRun(input);
			if (this.runs) this.runs = { ...this.runs, clips: [created, ...this.runs.clips] };
			this.importOpen = false;
			this.select(created);
		} catch (error) {
			this.importError = errorMessage(error);
		} finally {
			this.importBusy = null;
		}
	}

	async importTheElite(username: string): Promise<void> {
		this.importBusy = 'elite';
		this.importError = null;
		this.importResult = null;
		try {
			this.importResult = await this.api.importTheElite(username);
			await this.reload();
		} catch (error) {
			this.importError = errorMessage(error);
		} finally {
			this.importBusy = null;
		}
	}

	confirmReadClips(): void {
		this.readClipsOpen = false;
		void this.reload(true);
	}

	cancelDelete(): void {
		if (!this.deleteBusy) this.deleteTarget = null;
	}

	private async rename(clip: RunClip | null, scope: ActionScope): Promise<void> {
		if (!clip?.path) return;
		const next = this.navigation.promptForFilename('New filename (extension preserved if omitted):', clip.fileName);
		if (next === null) return;
		const fileName = next.trim();
		if (!fileName || fileName === clip.fileName) return;
		await this.performAction(scope, clip, 'rename', async () => {
			const updated = await this.api.renameRun(clip.path, fileName);
			this.updateClip(updated);
			if (this.selected?.runId === clip.runId) this.selected = updated;
		});
	}

	private async reveal(clip: RunClip | null, scope: ActionScope): Promise<void> {
		if (!clip?.path) return;
		await this.performAction(scope, clip, 'reveal', () => this.api.revealRun(clip.path));
	}

	private async keep(clip: RunClip | null, scope: ActionScope): Promise<void> {
		if (!clip) return;
		await this.performAction(scope, clip, 'keep', async () => {
			const updated = await this.api.keepRun(clip.runId);
			this.updateClip(updated);
			if (this.selected?.runId === clip.runId) this.selected = updated;
		});
	}

	private async performAction(
		scope: ActionScope,
		clip: RunClip,
		busy: string,
		action: () => Promise<unknown>
	): Promise<void> {
		this.setActionState(scope, clip, busy, null);
		try {
			await action();
		} catch (error) {
			this.setActionState(scope, clip, null, errorMessage(error));
			return;
		}
		this.setActionState(scope, clip, null, null);
	}

	private setActionState(scope: ActionScope, clip: RunClip, busy: string | null, error: string | null): void {
		if (scope === 'modal') {
			this.modalBusy = busy;
			this.modalError = error;
		} else {
			this.listActionBusyId = busy ? clip.runId : null;
			this.listActionError = error;
		}
	}

	private openFolder(): void {
		if (this.revealableDirectories.length === 0) return;
		void this.revealRunsFolder('completed');
	}

	private async revealRunsFolder(kind: RunDirectoryScan['kind']): Promise<void> {
		this.folderRevealBusy = true;
		this.error = null;
		try {
			await this.api.revealRunFolder(kind);
		} catch (error) {
			this.error = errorMessage(error);
		} finally {
			this.folderRevealBusy = false;
		}
	}

	private normalizeDraftTime(): void {
		if (this.metadataDraft) this.metadataDraft.time = normalizeRunTimeInput(this.metadataDraft.time);
	}

	private updateClip(updated: RunClip): void {
		if (!this.runs) return;
		this.runs = {
			...this.runs,
			clips: this.runs.clips.map((candidate) => (candidate.runId === updated.runId ? updated : candidate))
		};
	}

	private removeClip(runId: string): void {
		if (!this.runs) return;
		this.runs = { ...this.runs, clips: this.runs.clips.filter((clip) => clip.runId !== runId) };
	}
}
