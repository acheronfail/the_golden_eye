import { describe, expect, it } from 'vitest';
import { formatWallClockTime, MonitorWallClocks, type AnimationClock } from './monitorWallClocks.svelte';

class FakeAnimationClock implements AnimationClock {
	time = 0;
	private callback: FrameRequestCallback | null = null;

	now = (): number => this.time;
	requestFrame = (callback: FrameRequestCallback): number => {
		this.callback = callback;
		return 1;
	};
	cancelFrame = (): void => {
		this.callback = null;
	};

	advance(milliseconds: number): void {
		this.time += milliseconds;
		const callback = this.callback;
		this.callback = null;
		callback?.(this.time);
	}
}

describe('MonitorWallClocks', () => {
	it('counts session wall time from the monitoring rising edge and preserves it when stopped', () => {
		const clock = new FakeAnimationClock();
		const timers = new MonitorWallClocks(clock);

		timers.reconcile(false, null);
		expect(timers.snapshot()).toMatchObject({ sessionElapsedMs: 0, sessionRunning: false });

		timers.reconcile(true, 'unknown');
		clock.advance(61_234);
		expect(timers.snapshot()).toMatchObject({ sessionElapsedMs: 61_234, sessionRunning: true });

		timers.reconcile(false, 'unknown');
		clock.advance(2_000);
		expect(timers.snapshot()).toMatchObject({ sessionElapsedMs: 61_234, sessionRunning: false });
	});

	it('starts level time only on start to unknown and stops on the next known screen', () => {
		const clock = new FakeAnimationClock();
		const timers = new MonitorWallClocks(clock);

		timers.reconcile(true, 'start');
		clock.advance(500);
		expect(timers.snapshot()).toMatchObject({ levelElapsedMs: 0, levelRunning: false });

		timers.reconcile(true, 'unknown');
		clock.advance(2_345);
		expect(timers.snapshot()).toMatchObject({ levelElapsedMs: 2_345, levelRunning: true });

		timers.reconcile(true, 'stats');
		clock.advance(1_000);
		expect(timers.snapshot()).toMatchObject({ levelElapsedMs: 2_345, levelRunning: false });

		timers.reconcile(true, 'unknown');
		clock.advance(1_000);
		expect(timers.snapshot()).toMatchObject({ levelElapsedMs: 2_345, levelRunning: false });
	});

	it('accepts the title-cased screen values emitted by the production matcher', () => {
		const clock = new FakeAnimationClock();
		const timers = new MonitorWallClocks(clock);

		timers.reconcile(true, 'Start');
		timers.reconcile(true, 'Unknown');
		clock.advance(1_250);

		expect(timers.snapshot()).toMatchObject({ levelElapsedMs: 1_250, levelRunning: true });
	});

	it('stays stopped when a known level screen follows start and resets on the next start', () => {
		const clock = new FakeAnimationClock();
		const timers = new MonitorWallClocks(clock);

		timers.reconcile(true, 'start');
		timers.reconcile(true, 'level');
		timers.reconcile(true, 'unknown');
		clock.advance(900);
		expect(timers.snapshot()).toMatchObject({ levelElapsedMs: 0, levelRunning: false });

		timers.reconcile(true, 'start');
		timers.reconcile(true, 'unknown');
		clock.advance(400);
		expect(timers.snapshot()).toMatchObject({ levelElapsedMs: 400, levelRunning: true });

		timers.reconcile(true, 'start');
		expect(timers.snapshot()).toMatchObject({ levelElapsedMs: 0, levelRunning: false });
	});
});

describe('formatWallClockTime', () => {
	it('formats minutes, seconds, and milliseconds with fixed-width sub-minute fields', () => {
		expect(formatWallClockTime(0)).toBe('00:00:000');
		expect(formatWallClockTime(61_234)).toBe('01:01:234');
		expect(formatWallClockTime(6_001_009)).toBe('100:01:009');
	});
});
