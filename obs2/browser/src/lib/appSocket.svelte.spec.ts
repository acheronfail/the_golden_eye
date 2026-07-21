import { beforeEach, describe, expect, it } from 'vitest';
import { handleUpdateApplied } from './appSocket.svelte';
import { notifications } from './notifications.svelte';

describe('update-applied notifications', () => {
	beforeEach(() => {
		notifications.flags = [];
		sessionStorage.clear();
	});

	it('shows the event version and discards a persisted version from an earlier update', () => {
		sessionStorage.setItem('ge-update-applied-version', JSON.stringify({ version: '0.5.0' }));

		handleUpdateApplied('0.6.0-beta2');

		expect(notifications.flags).toHaveLength(1);
		expect(notifications.flags[0]).toMatchObject({
			title: 'Plugin updated',
			detail: 'Now running v0.6.0-beta2',
			tone: 'success'
		});
		expect(sessionStorage.getItem('ge-update-applied-version')).toBeNull();
	});
});
