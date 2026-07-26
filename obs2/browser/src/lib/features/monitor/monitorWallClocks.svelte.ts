export interface AnimationClock {
	now: () => number;
	requestFrame: (callback: FrameRequestCallback) => number;
	cancelFrame: (id: number) => void;
}

export interface MonitorWallClockSnapshot {
	sessionElapsedMs: number;
	sessionRunning: boolean;
	levelElapsedMs: number;
	levelRunning: boolean;
}

const browserClock: AnimationClock = {
	now: () => performance.now(),
	requestFrame: (callback) => requestAnimationFrame(callback),
	cancelFrame: (id) => cancelAnimationFrame(id)
};

export class MonitorWallClocks {
	sessionElapsedMs = $state(0);
	sessionRunning = $state(false);
	levelElapsedMs = $state(0);
	levelRunning = $state(false);

	private sessionStartedAt: number | null = null;
	private levelStartedAt: number | null = null;
	private levelArmed = false;
	private previousMonitoring = false;
	private previousScreen: string | null = null;
	private animationFrame: number | null = null;

	constructor(private readonly clock: AnimationClock = browserClock) {}

	reconcile(monitoring: boolean, screen: string | null): void {
		const now = this.clock.now();
		const normalizedScreen = screen?.trim().toLowerCase() ?? null;

		if (monitoring && !this.previousMonitoring) {
			this.sessionElapsedMs = 0;
			this.sessionStartedAt = now;
			this.sessionRunning = true;
			this.levelElapsedMs = 0;
			this.levelStartedAt = null;
			this.levelRunning = false;
			this.levelArmed = false;
			this.previousScreen = null;
		} else if (!monitoring && this.previousMonitoring) {
			this.tick(now);
			this.sessionRunning = false;
			this.sessionStartedAt = null;
			this.stopLevel(now);
		}

		if (monitoring && normalizedScreen !== this.previousScreen) {
			this.reconcileScreen(normalizedScreen, now);
			this.previousScreen = normalizedScreen;
		}
		this.previousMonitoring = monitoring;
		this.ensureAnimation();
	}

	snapshot(): MonitorWallClockSnapshot {
		return {
			sessionElapsedMs: this.sessionElapsedMs,
			sessionRunning: this.sessionRunning,
			levelElapsedMs: this.levelElapsedMs,
			levelRunning: this.levelRunning
		};
	}

	destroy(): void {
		if (this.animationFrame != null) this.clock.cancelFrame(this.animationFrame);
		this.animationFrame = null;
	}

	private reconcileScreen(screen: string | null, now: number): void {
		if (screen === 'start') {
			this.levelElapsedMs = 0;
			this.levelStartedAt = null;
			this.levelRunning = false;
			this.levelArmed = true;
			return;
		}
		if (screen === 'unknown' && this.levelArmed) {
			this.levelStartedAt = now;
			this.levelRunning = true;
			this.levelArmed = false;
			return;
		}
		if (screen !== 'unknown') {
			this.stopLevel(now);
			this.levelArmed = false;
		}
	}

	private stopLevel(now: number): void {
		if (this.levelRunning) this.levelElapsedMs = this.elapsedSince(this.levelStartedAt, now);
		this.levelRunning = false;
		this.levelStartedAt = null;
	}

	private tick(now: number): void {
		if (this.sessionRunning) this.sessionElapsedMs = this.elapsedSince(this.sessionStartedAt, now);
		if (this.levelRunning) this.levelElapsedMs = this.elapsedSince(this.levelStartedAt, now);
	}

	private elapsedSince(startedAt: number | null, now: number): number {
		return startedAt == null ? 0 : Math.max(0, Math.floor(now - startedAt));
	}

	private ensureAnimation(): void {
		if (this.animationFrame != null || (!this.sessionRunning && !this.levelRunning)) return;
		this.animationFrame = this.clock.requestFrame((now) => {
			this.animationFrame = null;
			this.tick(now);
			this.ensureAnimation();
		});
	}
}

export const formatWallClockTime = (milliseconds: number): string => {
	const wholeMilliseconds = Math.max(0, Math.floor(milliseconds));
	const minutes = Math.floor(wholeMilliseconds / 60_000);
	const seconds = Math.floor((wholeMilliseconds % 60_000) / 1_000);
	const millis = wholeMilliseconds % 1_000;
	return `${minutes.toString().padStart(2, '0')}:${seconds.toString().padStart(2, '0')}:${millis
		.toString()
		.padStart(3, '0')}`;
};
