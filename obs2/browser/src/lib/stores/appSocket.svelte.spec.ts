import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import type { AppEvent } from '$lib/api';
import { startAppSocket, stopAppSocket } from '$lib/stores/appSocket.svelte';
import { youtube } from '$lib/stores/youtube.svelte';

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
});
