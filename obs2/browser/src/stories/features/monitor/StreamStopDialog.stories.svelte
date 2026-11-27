<script module lang="ts">
	import { defineMeta } from '@storybook/addon-svelte-csf';
	import { expect, userEvent, within } from 'storybook/test';
	import StreamStopDialog from '$lib/features/monitor/StreamStopDialog.svelte';

	const { Story } = defineMeta({
		title: 'Monitor/Stop streaming dialog',
		component: StreamStopDialog,
		parameters: { layout: 'fullscreen' },
		args: { onClose: () => {}, stopStream: async () => {} }
	});
</script>

<Story name="Confirmation" />
<Story
	name="Stopping"
	args={{ stopStream: () => new Promise<void>(() => {}) }}
	play={async ({ canvasElement }) => {
		const canvas = within(canvasElement);
		await userEvent.click(canvas.getByRole('button', { name: 'Stop streaming' }));
		await expect(canvas.getByRole('button', { name: 'Stopping…' })).toBeDisabled();
	}}
/>
<Story
	name="Stop failed"
	args={{
		stopStream: async () => {
			throw new Error('Could not reach OBS.');
		}
	}}
	play={async ({ canvasElement }) => {
		const canvas = within(canvasElement);
		await userEvent.click(canvas.getByRole('button', { name: 'Stop streaming' }));
		await expect(canvas.getByRole('alert')).toHaveTextContent('Could not reach OBS.');
	}}
/>
