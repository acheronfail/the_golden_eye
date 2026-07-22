import { backend, type RunClip } from '$lib/api';

export const failedReview = new (class {
	clips = $state<RunClip[]>([]);
	open = $state(false);
	loading = $state(false);
	busy = $state(false);
	error = $state<string | null>(null);
	private revealWhenAvailable = false;
	private loadQueued = false;

	showWhenAvailable(): void {
		this.revealWhenAvailable = true;
		void this.load(true);
	}

	refresh(): void {
		void this.load(this.revealWhenAvailable);
	}

	monitorStarted(): void {
		this.revealWhenAvailable = false;
		this.open = false;
	}

	async load(openWhenFound = false): Promise<void> {
		if (this.loading || this.busy) {
			this.loadQueued = true;
			return;
		}
		this.loading = true;
		this.error = null;
		try {
			this.clips = await backend.getPendingFailedReviews();
			if (openWhenFound && this.revealWhenAvailable && this.clips.length > 0) this.open = true;
			if (this.clips.length === 0) this.open = false;
		} catch (err) {
			this.error = err instanceof Error ? err.message : String(err);
			if (openWhenFound && this.revealWhenAvailable) this.open = true;
		} finally {
			this.loading = false;
			if (this.loadQueued) {
				this.loadQueued = false;
				void this.load(this.revealWhenAvailable);
			}
		}
	}

	async keep(paths: string[]): Promise<void> {
		await this.apply(paths, (selected) => backend.keepFailedReviews(selected));
	}

	async discard(paths: string[]): Promise<void> {
		await this.apply(paths, (selected) => backend.discardFailedReviews(selected));
	}

	close(): void {
		this.revealWhenAvailable = false;
		this.open = false;
	}

	private async apply(paths: string[], action: (paths: string[]) => Promise<void>): Promise<void> {
		if (paths.length === 0 || this.busy) return;
		this.busy = true;
		this.error = null;
		try {
			await action(paths);
			const handled = new Set(paths);
			this.clips = this.clips.filter((clip) => !handled.has(clip.path));
			if (this.clips.length === 0) this.open = false;
		} catch (err) {
			this.error = err instanceof Error ? err.message : String(err);
		} finally {
			this.busy = false;
		}
	}
})();
