<script module lang="ts">
	import { defineMeta } from '@storybook/addon-svelte-csf';
	import YouTubeOptionsStory from './YouTubeOptionsStory.svelte';
	import { connectedYouTube, youtubeStatus } from './fixtures';

	const { Story } = defineMeta({
		title: 'Options/YouTube OAuth',
		component: YouTubeOptionsStory,
		parameters: { layout: 'fullscreen' },
		args: { status: youtubeStatus() }
	});
</script>

<Story name="OAuth unavailable" args={{ status: youtubeStatus({ oauthConfigured: false }) }} />
<Story name="Ready to connect" />
<Story name="Waiting for browser" args={{ connecting: true }} />
<Story name="Connection failed" args={{ error: 'The OAuth callback timed out. Cancel and try connecting again.' }} />
<Story name="Connected" args={{ status: connectedYouTube }} />
<Story name="Disconnecting" args={{ status: connectedYouTube, disconnecting: true }} />
