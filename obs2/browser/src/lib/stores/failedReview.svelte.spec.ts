import { backend, type RunClip } from '$lib/api';
import { failedReview } from '$lib/stores/failedReview.svelte';
import { beforeEach, describe, expect, it, vi } from 'vitest';

vi.mock('$lib/api', () => ({
	backend: {
		getPendingFailedReviews: vi.fn()
	}
}));

const clip = { path: '/failed/run.mkv' } as RunClip;

describe('failed review store', () => {
	beforeEach(() => {
		failedReview.close();
		failedReview.clips = [];
		failedReview.loading = false;
		failedReview.busy = false;
		failedReview.error = null;
		vi.mocked(backend.getPendingFailedReviews).mockReset();
	});

	it('does not reopen after monitoring starts during a pending load', async () => {
		let resolve!: (clips: RunClip[]) => void;
		vi.mocked(backend.getPendingFailedReviews).mockReturnValue(
			new Promise((done) => {
				resolve = done;
			})
		);

		failedReview.showWhenAvailable();
		failedReview.monitorStarted();
		resolve([clip]);
		await vi.waitFor(() => expect(failedReview.loading).toBe(false));

		expect(failedReview.open).toBe(false);
		expect(failedReview.clips).toEqual([clip]);
	});
});
