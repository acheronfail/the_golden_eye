<script module lang="ts">
	import { defineMeta } from '@storybook/addon-svelte-csf';
	import SectionTitleStory from './SectionTitleStory.svelte';

	const { Story } = defineMeta({
		title: 'Components/Section title',
		component: SectionTitleStory,
		parameters: { layout: 'fullscreen' },
		args: { title: 'Metadata', withAction: false }
	});
</script>

<Story name="With detail" args={{ title: 'Today', detail: '4 runs' }} />

<Story name="Default" />
<Story name="With action" args={{ title: 'YouTube', withAction: true }} />
<Story name="Source section" args={{ title: 'Last used source' }} />
