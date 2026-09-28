<script module lang="ts">
	import { expect, within } from 'storybook/test';
	import { defineMeta } from '@storybook/addon-svelte-csf';
	import RunDetailStory from './RunDetailStory.svelte';
	import {
		completedRun,
		connectedYouTube,
		uploadForRun,
		failedRun,
		manuallyLinkedYouTubeRun,
		pendingRun,
		theEliteRun,
		theEliteRunWithoutVideo,
		untaggedRun,
		youtubeStatus
	} from '../../fixtures';

	const { Story } = defineMeta({
		title: 'Runs/Run modal',
		component: RunDetailStory,
		parameters: { layout: 'fullscreen' },
		args: { clip: completedRun, status: youtubeStatus({ enabled: false }) }
	});
</script>

<Story name="Completed run" />
<Story name="Pending cleanup" args={{ clip: pendingRun }} />
<Story name="Failed run" args={{ clip: failedRun }} />
<Story name="Missing metadata" args={{ clip: untaggedRun }} />
<Story name="The Elite NTSC-J YouTube run" args={{ clip: theEliteRun }} />
<Story name="Manually linked YouTube run" args={{ clip: manuallyLinkedYouTubeRun }} />
<Story name="The Elite run without video" args={{ clip: theEliteRunWithoutVideo }} />
<Story name="Saving metadata" args={{ modalBusy: 'metadata' }} />
<Story name="Deleting" args={{ modalBusy: 'delete' }} />
<Story name="Update failed" args={{ modalError: 'The clip was moved or renamed outside the plugin.' }} />

<Story
	name="Uploading to YouTube"
	args={{ status: { ...connectedYouTube, uploads: [uploadForRun('uploading')] } }}
	play={async ({ canvasElement }) => {
		const canvas = within(canvasElement);
		const progress = await canvas.findByRole('progressbar', { name: 'YouTube upload progress' });
		const editTemplates = canvas.getByRole('link', { name: 'Edit templates' });
		const upload = canvas.getByRole('button', { name: /Uploading/ });
		const title = canvas.getByText('Title', { selector: 'dt' });
		await expect(upload).toBeDisabled();
		await expect(progress.getBoundingClientRect().top).toBeGreaterThan(editTemplates.getBoundingClientRect().bottom);
		await expect(progress.getBoundingClientRect().top).toBeGreaterThan(upload.getBoundingClientRect().bottom);
		await expect(progress.getBoundingClientRect().bottom).toBeLessThan(title.getBoundingClientRect().top);
	}}
/>
