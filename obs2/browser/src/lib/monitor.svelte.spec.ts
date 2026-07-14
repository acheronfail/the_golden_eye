import { beforeEach, describe, expect, it } from 'vitest';

import type { RecordingSaved, RecordingSavePending } from './api';
import {
	applyFailedRunNotSaved,
	applyRecordingSaved,
	applyRecordingSaveDiscarded,
	applyRecordingSavePending
} from './monitor.svelte';
import { notifications } from './notifications.svelte';

const pending = (overrides: Partial<RecordingSavePending> = {}): RecordingSavePending => ({
	saveId: 1,
	saveInSecs: 5,
	estimatedDurationSecs: 20,
	failed: true,
	status: 'kia',
	level: 'Runway',
	difficulty: '00 Agent',
	timeSecs: 374,
	...overrides
});

const saved = (overrides: Partial<RecordingSaved> = {}): RecordingSaved => ({
	saveId: 1,
	path: '/clips/runway.mov',
	replayPath: '/clips/replay.mov',
	durationSecs: 12.3,
	failed: true,
	...overrides
});

describe('recording save notifications', () => {
	beforeEach(() => {
		notifications.flags = [];
	});

	it('replaces the pending notification in place when the run time is refined', () => {
		applyRecordingSavePending(pending({ timeSecs: 374 }));
		expect(notifications.flags).toHaveLength(1);
		const firstId = notifications.flags[0].id;
		expect(notifications.flags[0].detail).toContain('6:14');

		// Same saveId, corrected time: one toast, updated in place (no stale twin).
		applyRecordingSavePending(pending({ timeSecs: 14 }));
		expect(notifications.flags).toHaveLength(1);
		expect(notifications.flags[0].id).toBe(firstId);
		expect(notifications.flags[0].detail).toContain('0:14');
	});

	it('resolves the refined pending notification into the saved toast', () => {
		applyRecordingSavePending(pending({ timeSecs: 374 }));
		applyRecordingSavePending(pending({ timeSecs: 14 }));
		const pendingId = notifications.flags[0].id;

		applyRecordingSaved(saved());
		expect(notifications.flags).toHaveLength(1);
		expect(notifications.flags[0].id).toBe(pendingId);
		expect(notifications.flags[0].title).toBe('Clip saved');
	});

	it('dismisses the pending notification when the save is discarded', () => {
		applyRecordingSavePending(pending());
		expect(notifications.flags).toHaveLength(1);

		applyRecordingSaveDiscarded({ saveId: 1 });
		expect(notifications.flags).toHaveLength(0);
	});

	it('ignores a discard for a save with no pending notification', () => {
		applyRecordingSaveDiscarded({ saveId: 99 });
		expect(notifications.flags).toHaveLength(0);
	});

	it('surfaces a failed-run-not-saved outcome as a transient notification', () => {
		applyFailedRunNotSaved('tooShort');
		expect(notifications.flags).toHaveLength(1);
		expect(notifications.flags[0].title).toBe('Failed run not saved');
		expect(notifications.flags[0].detail).toContain('minimum');
		// Auto-dismissing, not a sticky phase indicator.
		expect(notifications.flags[0].timeoutMs).toBeGreaterThan(0);

		applyFailedRunNotSaved('savingDisabled');
		expect(notifications.flags).toHaveLength(2);
		expect(notifications.flags[1].detail).toContain('turned off');
	});
});
