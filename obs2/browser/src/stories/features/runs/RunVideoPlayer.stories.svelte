<script module lang="ts">
	import { defineMeta } from '@storybook/addon-svelte-csf';
	import { expect, fn, userEvent, within } from 'storybook/test';
	import RunVideoPlayer from '$lib/features/runs/RunVideoPlayer.svelte';

	const { Story } = defineMeta({
		title: 'Runs/Video player',
		component: RunVideoPlayer,
		args: { src: '', fileBrowserLabel: 'Show in file browser', onReveal: fn() }
	});
</script>

<Story name="Default" />
<Story
	name="Unsupported video"
	play={async ({ canvasElement, args }) => {
		const video = canvasElement.querySelector('video')!;
		Object.defineProperties(video, {
			readyState: { configurable: true, value: 2 },
			videoWidth: { configurable: true, value: 0 },
			videoHeight: { configurable: true, value: 0 }
		});
		video.dispatchEvent(new Event('loadeddata'));
		const canvas = within(canvasElement);
		await expect(await canvas.findByRole('alert')).toHaveTextContent('Video preview unavailable');
		await userEvent.click(canvas.getByRole('button', { name: args.fileBrowserLabel }));
		await expect(args.onReveal).toHaveBeenCalled();
	}}
/>
<Story
	name="Load failed"
	play={async ({ canvasElement }) => {
		const video = canvasElement.querySelector('video')!;
		Object.defineProperty(video, 'error', { configurable: true, value: { code: 2 } });
		video.dispatchEvent(new Event('error'));
		await expect(await within(canvasElement).findByRole('alert')).toHaveTextContent('could not be loaded');
	}}
/>
