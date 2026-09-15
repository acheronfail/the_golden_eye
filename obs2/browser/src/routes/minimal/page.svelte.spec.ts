import { fireEvent, render, screen } from '@testing-library/svelte';
import { beforeEach, afterEach, describe, expect, it, vi } from 'vitest';
import { tick } from 'svelte';
import { backend, type AppEvent } from '$lib/api';
import Page from './+page.svelte';

const key = 'the-golden-eye.minimal.font-size';
let receive: (event: AppEvent) => void;
let disconnect: () => void;
const close = vi.fn();

beforeEach(() => {
	localStorage.clear();
	vi.spyOn(backend, 'connectAppSocket').mockImplementation((onEvent, onClose) => {
		receive = onEvent;
		disconnect = onClose;
		return { close } as unknown as WebSocket;
	});
});

afterEach(() => {
	vi.restoreAllMocks();
	vi.useRealTimers();
});

describe('minimal overlay', () => {
	it('toggles times with T and restores the preference on remount', async () => {
		const page = render(Page);
		expect(screen.getByLabelText('Recognised times')).toBeInTheDocument();
		await fireEvent.keyDown(window, { key: 't' });
		expect(screen.queryByLabelText('Recognised times')).not.toBeInTheDocument();
		expect(localStorage.getItem('the-golden-eye.minimal.show-times')).toBe('false');
		page.unmount();
		render(Page);
		expect(screen.queryByLabelText('Recognised times')).not.toBeInTheDocument();
		await fireEvent.keyDown(window, { key: 'T', repeat: true });
		expect(screen.queryByLabelText('Recognised times')).not.toBeInTheDocument();
		await fireEvent.keyDown(window, { key: 'T' });
		expect(screen.getByLabelText('Recognised times')).toBeInTheDocument();
		expect(localStorage.getItem('the-golden-eye.minimal.show-times')).toBe('true');
	});

	it('starts the last-used source and stops the active monitor with Space', async () => {
		const start = vi.spyOn(backend, 'startMonitor').mockResolvedValue();
		const stop = vi.spyOn(backend, 'stopMonitor').mockResolvedValue();
		render(Page);
		receive({
			type: 'snapshot',
			state: { monitor: { enabled: false }, settingsStatus: { settings: { lastUsedSourceName: 'N64' } } }
		} as AppEvent);
		await fireEvent.keyDown(window, { key: ' ' });
		expect(start).toHaveBeenCalledWith('N64');
		receive({
			type: 'snapshot',
			state: { monitor: { enabled: true, sourceName: 'N64' }, recordingState: 'started' }
		} as AppEvent);
		await fireEvent.keyDown(window, { key: ' ' });
		expect(stop).toHaveBeenCalledOnce();
	});

	it('ignores disconnected, repeated, and pending Space presses and allows retry after failure', async () => {
		let rejectStart!: (error: Error) => void;
		const start = vi.spyOn(backend, 'startMonitor').mockImplementation(
			() =>
				new Promise((_, reject) => {
					rejectStart = reject;
				})
		);
		render(Page);
		await fireEvent.keyDown(window, { key: ' ' });
		expect(start).not.toHaveBeenCalled();
		receive({
			type: 'snapshot',
			state: { monitor: { enabled: false }, settingsStatus: { settings: { lastUsedSourceName: 'N64' } } }
		} as AppEvent);
		await fireEvent.keyDown(window, { key: ' ', repeat: true });
		expect(start).not.toHaveBeenCalled();
		await fireEvent.keyDown(window, { key: ' ' });
		await fireEvent.keyDown(window, { key: ' ' });
		expect(start).toHaveBeenCalledOnce();
		expect(screen.getByRole('status')).toHaveTextContent('Starting monitor');
		rejectStart(new Error('Replay buffer is unavailable'));
		await tick();
		expect(screen.getByRole('status')).toHaveTextContent('Replay buffer is unavailable');
		start.mockResolvedValue();
		await fireEvent.keyDown(window, { key: ' ' });
		expect(start).toHaveBeenCalledTimes(2);
	});

	it('explains when no capture source is selected', async () => {
		const start = vi.spyOn(backend, 'startMonitor').mockResolvedValue();
		render(Page);
		receive({ type: 'snapshot', state: { monitor: { enabled: false } } } as AppEvent);
		await fireEvent.keyDown(window, { key: ' ' });
		expect(start).not.toHaveBeenCalled();
		expect(screen.getByRole('status')).toHaveTextContent('Choose a source in the main app');
	});

	it('keeps three time slots visible and clears times when they leave the screen', async () => {
		render(Page);
		const values = () =>
			['Run time', 'Target time', 'Best time'].map((label) => screen.getByLabelText(label).textContent);
		const snapshot = async (times: { time: number; target_time: number | null; best_time: number | null } | null) => {
			receive({
				type: 'snapshot',
				state: { monitor: { enabled: true }, recordingState: 'complete', match: { times } }
			} as AppEvent);
			await tick();
		};
		expect(values()).toEqual(['--:--', '--:--', '--:--']);
		await snapshot({ time: 83, target_time: 120, best_time: 75 });
		expect(values()).toEqual(['01:23', '02:00', '01:15']);
		await snapshot({ time: 0, target_time: null, best_time: null });
		expect(values()).toEqual(['00:00', '--:--', '--:--']);
		await snapshot(null);
		expect(values()).toEqual(['--:--', '--:--', '--:--']);
		await snapshot({ time: 83, target_time: 120, best_time: 75 });
		receive({ type: 'monitorStopped', reason: 'replayBufferStopped' } as AppEvent);
		await tick();
		expect(values()).toEqual(['--:--', '--:--', '--:--']);
		await snapshot({ time: 83, target_time: 120, best_time: 75 });
		disconnect();
		await tick();
		expect(values()).toEqual(['--:--', '--:--', '--:--']);
	});

	it('resizes the entire overlay and restores its saved size on remount', async () => {
		const first = render(Page);
		expect(screen.getByRole('main').style.getPropertyValue('--overlay-font-size')).toBe('16px');
		await fireEvent.keyDown(window, { key: 'ArrowUp' });
		expect(localStorage.getItem(key)).toBe('17');
		expect(screen.getByRole('main').style.getPropertyValue('--overlay-font-size')).toBe('17px');
		first.unmount();
		render(Page);
		expect(screen.getByRole('main').style.getPropertyValue('--overlay-font-size')).toBe('17px');
		await fireEvent.keyDown(window, { key: 'ArrowDown' });
		expect(localStorage.getItem(key)).toBe('16');
	});

	it('ignores invalid storage and keeps sizing usable when storage is blocked', async () => {
		localStorage.setItem(key, 'NaN');
		render(Page);
		expect(screen.getByRole('main').style.getPropertyValue('--overlay-font-size')).toBe('16px');
		vi.spyOn(Storage.prototype, 'setItem').mockImplementation(() => {
			throw new Error('blocked');
		});
		await fireEvent.keyDown(window, { key: 'ArrowUp' });
		expect(screen.getByRole('main').style.getPropertyValue('--overlay-font-size')).toBe('17px');
	});

	it('shows live status, reconnects after disconnect, and closes on unmount', async () => {
		vi.useFakeTimers();
		const page = render(Page);
		receive({ type: 'snapshot', state: { monitor: { enabled: true }, recordingState: 'started' } } as AppEvent);
		await tick();
		expect(screen.getByRole('status')).toHaveTextContent('Recording');
		receive({ type: 'monitorStopped', reason: 'replayBufferStopped' } as AppEvent);
		await tick();
		expect(screen.getByRole('status')).toHaveTextContent('Not monitoring');
		disconnect();
		await tick();
		expect(screen.getByRole('status')).toHaveTextContent('Disconnected');
		await vi.advanceTimersByTimeAsync(1000);
		expect(backend.connectAppSocket).toHaveBeenCalledTimes(2);
		page.unmount();
		expect(close).toHaveBeenCalled();
	});
});
