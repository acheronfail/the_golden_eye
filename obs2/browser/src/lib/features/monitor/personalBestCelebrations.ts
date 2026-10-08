import type { Backend } from '$lib/api';

export class PersonalBestCelebrations {
	private seen = new Set<string>();

	constructor(
		private api: Pick<Backend, 'getRuns'>,
		private source: () => string | null,
		private celebrate: () => void
	) {}

	async handle(event: { runId?: string; saveId?: number }): Promise<void> {
		const source = this.source();
		const runId = event.runId;
		if (!source || !runId || event.saveId === undefined || this.seen.has(runId)) return;
		this.seen.add(runId);
		if (this.seen.size > 256) this.seen.delete(this.seen.values().next().value!);
		try {
			const response = await this.api.getRuns({ runId, limit: 1 });
			const run = response.requestedRun ?? response.clips.find((run) => run.runId === runId);
			if (
				this.source() === source &&
				run?.runId === runId &&
				run.metadata.sourceName === source &&
				run.metadata.status === 'complete' &&
				run.metadata.wasPersonalBest
			)
				this.celebrate();
		} catch {
			this.seen.delete(runId);
		}
	}
}
