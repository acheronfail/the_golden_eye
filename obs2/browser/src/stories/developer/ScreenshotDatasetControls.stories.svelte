<script module lang="ts">
	import { defineMeta } from '@storybook/addon-svelte-csf';
	import ScreenshotDatasetControls from '$lib/developer/ScreenshotDatasetControls.svelte';

	const preview =
		'data:image/svg+xml,%3Csvg xmlns="http://www.w3.org/2000/svg" width="640" height="360"%3E%3Crect width="640" height="360" fill="%23101620"/%3E%3C/svg%3E';

	const { Story } = defineMeta({
		title: 'Developer/Screenshot Dataset Controls',
		component: ScreenshotDatasetControls,
		args: {
			imageData: preview,
			language: 'en',
			close: () => {}
		}
	});
</script>

<Story name="English" />
<Story name="Japanese streaming" args={{ language: 'jp', streaming: true }} />
