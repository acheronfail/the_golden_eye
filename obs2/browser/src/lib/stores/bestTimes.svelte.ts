import { backend, type RunClip } from '$lib/api';

export class BestTimesStore {
	items = $state<RunClip[]>([]);
	private refreshVersion = 0;

	async refresh(): Promise<void> {
		const version = ++this.refreshVersion;
		try {
			const items = await backend.getBestTimes();
			if (version === this.refreshVersion) this.items = items;
		} catch {
			if (version === this.refreshVersion) this.items = [];
		}
	}
}

export const bestTimes = new BestTimesStore();
