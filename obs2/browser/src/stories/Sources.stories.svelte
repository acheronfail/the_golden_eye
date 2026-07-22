<script module lang="ts">
	import { defineMeta } from '@storybook/addon-svelte-csf';
	import SourceSelectionStory from './SourceSelectionStory.svelte';

	const { Story } = defineMeta({
		title: 'Monitor/Source previews',
		component: SourceSelectionStory,
		parameters: { layout: 'fullscreen' },
		args: { state: 'available', previews: true, disabled: false, lastUsed: true }
	});
</script>

<Story name="Loading sources" args={{ state: 'loading' }} />
<Story name="No sources" args={{ state: 'empty' }} />
<Story name="Last used source" />
<Story name="No last used source" args={{ lastUsed: false }} />
<Story name="Preview missing" args={{ state: 'missing' }} />
<Story name="Mixed preview results" args={{ state: 'mixed' }} />
<Story name="Previews hidden" args={{ previews: false }} />
<Story name="Replay buffer unavailable" args={{ disabled: true }} />
