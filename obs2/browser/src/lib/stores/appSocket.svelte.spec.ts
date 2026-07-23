import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import type { AppEvent } from '$lib/api';
import { startAppSocket, stopAppSocket } from '$lib/stores/appSocket.svelte';
import { notifications } from '$lib/stores/notifications.svelte';
import { youtube } from '$lib/stores/youtube.svelte';
import { completedRun, uploadForRun } from '../../stories/fixtures';

const mocks = vi.hoisted(() => ({
	connectAppSocket: vi.fn()
}));

vi.mock('$lib/api', async (importOriginal) => {
	const actual = await importOriginal<typeof import('$lib/api')>();
	return {
		...actual,
		backend: {
			...actual.backend,
			connectAppSocket: mocks.connectAppSocket
		}
	};
});

let receiveEvent: ((event: AppEvent) => void) | undefined;

beforeEach(() => {
	youtube.loaded = true;
	youtube.enabled = true;
	youtube.oauthConfigured = true;
	youtube.connected = false;
	youtube.account = null;
	youtube.uploads = [];
	youtube.history = [];
	notifications.flags = [];
	mocks.connectAppSocket.mockImplementation((onEvent: (event: AppEvent) => void) => {
		receiveEvent = onEvent;
		return { close: vi.fn() };
	});
});

afterEach(() => {
	stopAppSocket();
	receiveEvent = undefined;
});

describe('app event socket', () => {
	it('applies YouTube status changes received from another browser client', () => {
		startAppSocket();

		receiveEvent?.({
			type: 'youtubeStatusChanged',
			status: {
				enabled: true,
				oauthConfigured: true,
				connected: true,
				account: { email: 'runner@example.com', name: 'Runner', picture: null },
				uploads: [],
				history: []
			}
		});

		expect(youtube.connected).toBe(true);
		expect(youtube.account?.email).toBe('runner@example.com');
	});

	it('links a failed YouTube upload notification to the run detail modal', () => {
		startAppSocket();

		receiveEvent?.({
			type: 'youtubeUploadChanged',
			upload: uploadForRun('failed', { id: 'failed-notification' })
		});

		expect(notifications.flags).toEqual([
			expect.objectContaining({
				title: 'YouTube upload failed',
				href: `/runs?runId=${encodeURIComponent(completedRun.runId)}`,
				timeoutMs: 8000
			})
		]);
	});

	it('does not notify when a YouTube upload starts or completes', () => {
		startAppSocket();

		receiveEvent?.({ type: 'youtubeUploadChanged', upload: uploadForRun('queued', { id: 'queued-notification' }) });
		receiveEvent?.({ type: 'youtubeUploadChanged', upload: uploadForRun('uploaded', { id: 'uploaded-notification' }) });

		expect(notifications.flags).toEqual([]);
	});
});
