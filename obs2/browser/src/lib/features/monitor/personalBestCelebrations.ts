import type { Backend, RecordingSavePending } from '$lib/api';

export class PersonalBestCelebrations {
	private generation = 0;
	private seen = new Set<string>();
	private celebratedSaves = new Set<number>();
	private pending = new Map<
		number,
		{ event: RecordingSavePending; source: string; timer: ReturnType<typeof setTimeout> }
	>();

	constructor(
		private api: Pick<Backend, 'getRuns' | 'getBestTimes'>,
		private source: () => string | null,
		private celebrate: () => void
	) {}

	reset(): void {
		this.generation += 1;
		for (const candidate of this.pending.values()) clearTimeout(candidate.timer);
		this.pending.clear();
		this.celebratedSaves.clear();
	}

	handlePending(event: RecordingSavePending): void {
		const previous = this.pending.get(event.saveId);
		const source = this.source();
		if (
			previous &&
			previous.source === source &&
			previous.event.timeSecs === event.timeSecs &&
			previous.event.levelNumber === event.levelNumber &&
			previous.event.difficulty === event.difficulty &&
			previous.event.status === event.status &&
			previous.event.failed === event.failed
		)
			return;
		if (previous) clearTimeout(previous.timer);
		this.pending.delete(event.saveId);
		if (
			!source ||
			this.celebratedSaves.has(event.saveId) ||
			event.failed ||
			event.status !== 'complete' ||
			event.timeSecs === undefined ||
			event.levelNumber === undefined ||
			event.levelNumber < 1 ||
			event.levelNumber > 20 ||
			!['Agent', 'Secret Agent', '00 Agent'].includes(event.difficulty ?? '')
		)
			return;
		const candidate = { event, source, timer: setTimeout(() => void this.checkPending(candidate), 1000) };
		this.pending.set(event.saveId, candidate);
	}

	private async checkPending(candidate: { event: RecordingSavePending; source: string }): Promise<void> {
		const { event, source } = candidate;
		try {
			const bestTimes = await this.api.getBestTimes();
			if (this.pending.get(event.saveId) !== candidate || this.source() !== source) return;
			const best = bestTimes.find(
				(run) => run.metadata.levelNumber === event.levelNumber && run.metadata.difficulty === event.difficulty
			);
			if (!best || (best.metadata.timeSeconds !== undefined && event.timeSecs! < best.metadata.timeSeconds)) {
				this.celebratedSaves.add(event.saveId);
				if (this.celebratedSaves.size > 256) this.celebratedSaves.delete(this.celebratedSaves.values().next().value!);
				this.celebrate();
			}
		} catch {
			// The finalized catalog event remains the fallback if this lookup fails.
		} finally {
			if (this.pending.get(event.saveId) === candidate) this.pending.delete(event.saveId);
		}
	}

	async handle(event: { runId?: string; saveId?: number }): Promise<void> {
		const source = this.source();
		const generation = this.generation;
		const runId = event.runId;
		if (!source || !runId || event.saveId === undefined || this.seen.has(runId)) return;
		const pending = this.pending.get(event.saveId);
		if (pending) clearTimeout(pending.timer);
		this.pending.delete(event.saveId);
		this.seen.add(runId);
		if (this.seen.size > 256) this.seen.delete(this.seen.values().next().value!);
		if (this.celebratedSaves.has(event.saveId)) return;
		try {
			const response = await this.api.getRuns({ runId, limit: 1 });
			const run = response.requestedRun ?? response.clips.find((run) => run.runId === runId);
			if (
				this.generation === generation &&
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
