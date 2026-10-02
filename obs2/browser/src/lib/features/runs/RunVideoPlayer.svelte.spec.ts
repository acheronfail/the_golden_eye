import { fireEvent, render, screen } from '@testing-library/svelte';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import RunVideoPlayer from './RunVideoPlayer.svelte';

function mediaState(video: HTMLVideoElement, width: number, readyState = 2) {
	Object.defineProperties(video, {
		readyState: { configurable: true, value: readyState },
		videoWidth: { configurable: true, value: width },
		videoHeight: { configurable: true, value: width ? 360 : 0 }
	});
}

function setup() {
	const onReveal = vi.fn();
	const result = render(RunVideoPlayer, { src: '/clip.mp4', fileBrowserLabel: 'Show in Finder', onReveal });
	return { ...result, onReveal, video: result.container.querySelector('video')! };
}

describe('RunVideoPlayer', () => {
	beforeEach(() => {
		vi.stubGlobal('MediaError', { MEDIA_ERR_ABORTED: 1, MEDIA_ERR_NETWORK: 2 });
	});
	afterEach(() => vi.unstubAllGlobals());

	it('warns about audio-only playback and reveals the file', async () => {
		const { video, onReveal } = setup();
		mediaState(video, 0);
		await fireEvent.loadedData(video);
		expect(screen.getByRole('alert')).toHaveTextContent(
			"OBS's built-in browser may not support this clip's video format"
		);
		await fireEvent.click(screen.getByRole('button', { name: 'Show in Finder' }));
		expect(onReveal).toHaveBeenCalledOnce();
	});

	it('does not warn while loading or when video is available', async () => {
		const { video } = setup();
		mediaState(video, 0, 0);
		await fireEvent.loadedData(video);
		expect(screen.queryByRole('alert')).toBeNull();
		mediaState(video, 640);
		await fireEvent.loadedData(video);
		expect(screen.queryByRole('alert')).toBeNull();
	});

	it('clears the warning when video becomes available or another clip opens', async () => {
		const { video, rerender } = setup();
		mediaState(video, 0);
		await fireEvent.loadedData(video);
		mediaState(video, 640);
		await fireEvent.resize(video);
		expect(screen.queryByRole('alert')).toBeNull();
		mediaState(video, 0);
		await fireEvent.loadedData(video);
		expect(screen.getByRole('alert')).toBeInTheDocument();
		await rerender({ src: '/other.mp4' });
		expect(screen.queryByRole('alert')).toBeNull();
	});

	it.each([2, 3, 4])('handles media error %s', async (code) => {
		const { video } = setup();
		Object.defineProperty(video, 'error', { value: { code } });
		await fireEvent.error(video);
		expect(screen.getByRole('alert')).toHaveTextContent(code === 2 ? 'could not be loaded' : 'may not support');
	});
});
