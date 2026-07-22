import { beforeEach, describe, expect, it, vi } from 'vitest';
import {
	addNotificationFlag,
	dismissNotificationFlag,
	notifications,
	removeNotificationFlag
} from '$lib/stores/notifications.svelte';

describe('notification dismissal callbacks', () => {
	beforeEach(() => {
		notifications.flags = [];
	});

	it('calls onDismiss only for user dismissals', () => {
		const onDismiss = vi.fn();
		const first = addNotificationFlag({ title: 'One', onDismiss, sticky: true });
		removeNotificationFlag(first.id);
		expect(onDismiss).not.toHaveBeenCalled();

		const second = addNotificationFlag({ title: 'Two', onDismiss, sticky: true });
		dismissNotificationFlag(second.id);
		expect(onDismiss).toHaveBeenCalledTimes(1);
		expect(notifications.flags).toHaveLength(0);
	});
});
