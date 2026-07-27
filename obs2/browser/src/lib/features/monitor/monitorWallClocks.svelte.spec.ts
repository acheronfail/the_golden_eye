import { describe, expect, it } from 'vitest';
import { formatWallClockTime, MonitorWallClocks, type AnimationClock } from './monitorWallClocks.svelte';

class FakeAnimationClock implements AnimationClock {
	time = 0;
	wallTime = 1_800_000_000_000;
	private callback: FrameRequestCallback | null = null;

	now = (): number => this.time;
	wallNow = (): number => this.wallTime;
	requestFrame = (callback: FrameRequestCallback): number => {
		this.callback = callback;
		return 1;
	};
	cancelFrame = (): void => {
		this.callback = null;
	};

	advance(milliseconds: number): void {
		this.time += milliseconds;
		this.wallTime += milliseconds;
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

	it('keeps fallback level time at zero while backend fade state is unavailable', () => {
		const clock = new FakeAnimationClock();
		const timers = new MonitorWallClocks(clock);

		timers.reconcile(true, 'start');
		clock.advance(500);
		expect(timers.snapshot()).toMatchObject({
			levelElapsedMs: 0,
			levelRunning: false,
			levelTimerPhase: 'awaitingInitialBlack'
		});

		timers.reconcile(true, 'unknown');
		clock.advance(2_345);
		expect(timers.snapshot()).toMatchObject({ levelElapsedMs: 0, levelRunning: false });

		timers.reconcile(true, 'stats');
		clock.advance(1_000);
		expect(timers.snapshot()).toMatchObject({
			levelElapsedMs: 0,
			levelRunning: false,
			levelTimerPhase: 'stopped'
		});

		timers.reconcile(true, 'unknown');
		clock.advance(1_000);
		expect(timers.snapshot()).toMatchObject({ levelElapsedMs: 0, levelRunning: false });
	});

	it('accepts the title-cased screen values emitted by the production matcher', () => {
		const clock = new FakeAnimationClock();
		const timers = new MonitorWallClocks(clock);

		timers.reconcile(true, 'Start');
		timers.reconcile(true, 'Unknown');
		clock.advance(1_250);

		expect(timers.snapshot()).toMatchObject({
			levelElapsedMs: 0,
			levelRunning: false,
			levelTimerPhase: 'awaitingInitialBlack'
		});
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
		expect(timers.snapshot()).toMatchObject({ levelElapsedMs: 0, levelRunning: false });

		timers.reconcile(true, 'start');
		expect(timers.snapshot()).toMatchObject({ levelElapsedMs: 0, levelRunning: false });
	});

	it('seeds running and stopped timers from backend timestamps after a reload', () => {
		const clock = new FakeAnimationClock();
		const timers = new MonitorWallClocks(clock);

		timers.sync({
			sessionStartedAtUnixMs: clock.wallTime - 61_234,
			sessionElapsedMs: 0,
			sessionRunning: true,
			levelStartedAtUnixMs: clock.wallTime - 2_345,
			levelElapsedMs: 0,
			levelRunning: true,
			levelStartReason: 'fade',
			levelTimerPhase: 'running',
			introSwirlDelayMs: 4_000,
			fadeDetection: {
				detected: false,
				meanLuma: 74,
				darkPixelPercent: 8,
				sampleCount: 576,
				sampleRegion: { x: 107, y: 0, width: 640, height: 480 }
			}
		});
		expect(timers.snapshot()).toMatchObject({
			sessionElapsedMs: 61_234,
			sessionRunning: true,
			levelElapsedMs: 2_345,
			levelRunning: true,
			levelTimerPhase: 'running',
			introSwirlDelayMs: 4_000
		});

		clock.advance(1_000);
		expect(timers.snapshot()).toMatchObject({
			sessionElapsedMs: 62_234,
			levelElapsedMs: 3_345
		});

		timers.sync({
			sessionStartedAtUnixMs: clock.wallTime - 62_234,
			sessionElapsedMs: 0,
			sessionRunning: true,
			levelStartedAtUnixMs: null,
			levelElapsedMs: 3_345,
			levelRunning: false,
			levelStartReason: 'fade',
			levelTimerPhase: 'stopped',
			introSwirlDelayMs: 4_000,
			fadeDetection: {
				detected: false,
				meanLuma: 74,
				darkPixelPercent: 8,
				sampleCount: 576,
				sampleRegion: { x: 107, y: 0, width: 640, height: 480 }
			}
		});
		clock.advance(1_000);
		expect(timers.snapshot()).toMatchObject({
			sessionElapsedMs: 63_234,
			sessionRunning: true,
			levelElapsedMs: 3_345,
			levelRunning: false
		});
	});
});

describe('formatWallClockTime', () => {
	it('formats minutes, seconds, and milliseconds with fixed-width sub-minute fields', () => {
		expect(formatWallClockTime(0)).toBe('00:00:000');
		expect(formatWallClockTime(61_234)).toBe('01:01:234');
		expect(formatWallClockTime(6_001_009)).toBe('100:01:009');
	});
});
