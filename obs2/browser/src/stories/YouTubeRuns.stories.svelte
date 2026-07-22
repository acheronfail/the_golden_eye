<script module lang="ts">
	import { defineMeta } from '@storybook/addon-svelte-csf';
	import RunYouTubeStory from './RunYouTubeStory.svelte';
	import { completedRun, connectedYouTube, uploadedHistory, uploadForRun, youtubeStatus } from './fixtures';

	const { Story } = defineMeta({
		title: 'Runs/YouTube OAuth and uploads',
		component: RunYouTubeStory,
		parameters: { layout: 'fullscreen' },
		args: { clip: completedRun, status: youtubeStatus() }
	});
</script>

<Story name="OAuth unavailable" args={{ status: youtubeStatus({ oauthConfigured: false }) }} />
<Story name="Ready to connect" />
<Story name="Waiting for browser" args={{ connecting: true }} />
<Story name="OAuth error" args={{ error: 'Access was denied by the Google account owner.' }} />
<Story name="Connected and ready" args={{ status: connectedYouTube }} />
<Story name="Upload queued" args={{ status: { ...connectedYouTube, uploads: [uploadForRun('queued')] } }} />
<Story name="Uploading" args={{ status: { ...connectedYouTube, uploads: [uploadForRun('uploading')] } }} />
<Story name="Processing" args={{ status: { ...connectedYouTube, uploads: [uploadForRun('processing')] } }} />
<Story name="Upload failed" args={{ status: { ...connectedYouTube, uploads: [uploadForRun('failed')] } }} />
<Story name="Uploaded" args={{ status: { ...connectedYouTube, uploads: [uploadForRun('uploaded')] } }} />
<Story name="Remembered upload" args={{ status: { ...connectedYouTube, history: [uploadedHistory] } }} />
