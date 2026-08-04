import type { MonitorStatus } from '$lib/api';
import type { MonitorTransition } from '$lib/features/monitor/MonitorView.svelte';

export type MonitorRoutePhase =
	| { kind: 'checking' }
	| { kind: 'verifying' }
	| { kind: 'starting' }
	| { kind: 'active'; confirmed: boolean }
	| { kind: 'stopping' }
	| { kind: 'redirecting'; href: string }
	| { kind: 'error'; operation: 'start' | 'stop'; message: string };

export type StopPromptState =
	| { kind: 'closed' }
	| { kind: 'open' }
	| { kind: 'saving' }
	| { kind: 'error'; message: string };

export interface MonitorSessionSnapshot {
	sourceName: string;
	currentPath: string;
	monitorLoaded: boolean;
	monitorStatus: MonitorStatus | null;
	sourcesLoaded: boolean;
	sourceExists: boolean;
}

export interface MonitorSessionActions {
	saveSettings: () => Promise<void>;
	refreshRecentRuns: () => Promise<unknown>;
	startMonitor: (sourceName: string) => Promise<unknown>;
	stopMonitor: () => Promise<unknown>;
	refreshReplayBuffer: () => void;
	navigate: (href: string, options?: { replaceState?: boolean }) => void;
	reportError: (message: string) => void;
	stopPromptShown: () => boolean;
	saveStopPreference: (stopReplayBuffer: boolean) => Promise<void>;
}

const errorMessage = (error: unknown): string => (error instanceof Error ? error.message : String(error));

export class MonitorSessionController {
	phase = $state<MonitorRoutePhase>({ kind: 'checking' });
	stopPrompt = $state<StopPromptState>({ kind: 'closed' });
	private operationId = 0;

	constructor(private readonly actions: MonitorSessionActions) {}

	get monitoring(): boolean {
		return this.phase.kind === 'active' || this.phase.kind === 'stopping';
	}

	get verified(): boolean {
		return this.phase.kind !== 'checking' && this.phase.kind !== 'verifying';
	}

	get transition(): MonitorTransition {
		if (this.phase.kind === 'starting') return 'starting';
		if (this.phase.kind === 'stopping') return 'stopping';
		return null;
	}

	get promptOpen(): boolean {
		return this.stopPrompt.kind !== 'closed';
	}

	get promptBusy(): boolean {
		return this.stopPrompt.kind === 'saving';
	}

	get promptError(): string | null {
		return this.stopPrompt.kind === 'error' ? this.stopPrompt.message : null;
	}

	reconcile(snapshot: MonitorSessionSnapshot): void {
		const sourcePath = `/sources/${encodeURIComponent(snapshot.sourceName)}`;
		if (snapshot.currentPath !== sourcePath || this.phase.kind === 'redirecting') return;
		if (!snapshot.monitorLoaded) {
			this.phase = { kind: 'checking' };
			return;
		}
		if (snapshot.monitorStatus?.enabled) {
			if (snapshot.monitorStatus.sourceName !== snapshot.sourceName) {
				this.redirect(`/sources/${encodeURIComponent(snapshot.monitorStatus.sourceName)}`);
				return;
			}
			if (this.phase.kind !== 'stopping') this.phase = { kind: 'active', confirmed: true };
			return;
		}
		if ((this.phase.kind === 'active' && this.phase.confirmed) || this.phase.kind === 'stopping') {
			this.redirect('/');
			return;
		}
		if (this.phase.kind === 'active') return;
		if (this.phase.kind === 'starting') return;
		if (!snapshot.sourcesLoaded) {
			this.phase = { kind: 'verifying' };
			return;
		}
		if (!snapshot.sourceExists) {
			this.redirect('/');
			return;
		}
		void this.start(snapshot.sourceName);
	}

	navigationSettled(): void {
		if (this.phase.kind === 'redirecting') this.phase = { kind: 'checking' };
	}

	requestStop(): void {
		if (!this.monitoring || this.transition || this.promptOpen) return;
		if (!this.actions.stopPromptShown()) {
			this.stopPrompt = { kind: 'open' };
			return;
		}
		void this.stop();
	}

	async chooseStopPreference(stopReplayBuffer: boolean): Promise<void> {
		if (this.stopPrompt.kind === 'closed') return;
		this.stopPrompt = { kind: 'saving' };
		try {
			await this.actions.saveStopPreference(stopReplayBuffer);
			this.stopPrompt = { kind: 'closed' };
			await this.stop();
		} catch (error) {
			this.stopPrompt = { kind: 'error', message: errorMessage(error) };
		}
	}

	handleKeydown(event: KeyboardEvent): void {
		if (this.transition || this.promptOpen || !this.monitoring) return;
		if (event.key === ' ' || event.key === 'Escape') {
			event.preventDefault();
			this.requestStop();
		}
	}

	private async start(sourceName: string): Promise<void> {
		if (this.phase.kind === 'starting' || this.phase.kind === 'active') return;
		const operationId = ++this.operationId;
		this.phase = { kind: 'starting' };
		try {
			await this.actions.saveSettings();
			await this.actions.refreshRecentRuns();
			await this.actions.startMonitor(sourceName);
			if (operationId !== this.operationId) return;
			this.actions.refreshReplayBuffer();
			this.phase = { kind: 'active', confirmed: false };
		} catch (error) {
			if (operationId !== this.operationId) return;
			const message = errorMessage(error);
			this.phase = { kind: 'error', operation: 'start', message };
			this.actions.reportError(message);
			this.redirect('/');
		}
	}

	private async stop(): Promise<void> {
		if (this.phase.kind !== 'active') return;
		const operationId = ++this.operationId;
		this.phase = { kind: 'stopping' };
		try {
			await this.actions.stopMonitor();
			if (operationId !== this.operationId) return;
			this.actions.refreshReplayBuffer();
			this.redirect('/');
		} catch (error) {
			if (operationId !== this.operationId) return;
			const message = errorMessage(error);
			this.phase = { kind: 'error', operation: 'stop', message };
			this.actions.reportError(message);
			this.phase = { kind: 'active', confirmed: true };
		}
	}

	private redirect(href: string): void {
		if (this.phase.kind === 'redirecting' && this.phase.href === href) return;
		this.operationId += 1;
		this.phase = { kind: 'redirecting', href };
		this.actions.navigate(href, { replaceState: true });
	}
}
