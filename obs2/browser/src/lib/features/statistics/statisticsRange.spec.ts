import { describe, expect, it } from 'vitest';
import { resolveDateRange } from './statisticsRange';

describe('resolveDateRange', () => {
	const now = new Date(2026, 6, 24, 12, 0, 0);

	it('uses local midnight for rolling presets', () => {
		const range = resolveDateRange({ preset: '7d', customFrom: '', customTo: '' }, now);
		expect(new Date(range.from!).getDate()).toBe(18);
		expect(range.to).toBe(now.toISOString());
	});

	it('makes a custom end date inclusive', () => {
		const range = resolveDateRange({ preset: 'custom', customFrom: '2026-07-01', customTo: '2026-07-24' }, now);
		expect(new Date(range.from!).getDate()).toBe(1);
		expect(new Date(range.to).getDate()).toBe(25);
	});

	it('rejects a reversed custom range', () => {
		const range = resolveDateRange({ preset: 'custom', customFrom: '2026-07-24', customTo: '2026-07-01' }, now);
		expect(range.error).toMatch(/Start date/);
	});

	it('rejects invalid custom calendar dates without throwing', () => {
		const range = resolveDateRange({ preset: 'custom', customFrom: '2026-02-30', customTo: '2026-03-01' }, now);

		expect(range.error).toBe('Choose valid start and end dates.');
		expect(range.from).toBeUndefined();
	});
});
