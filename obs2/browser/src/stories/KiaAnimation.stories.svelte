<script module lang="ts">
	import { defineMeta } from '@storybook/addon-svelte-csf';
	import KiaAnimationStory from './KiaAnimationStory.svelte';

	const { Story } = defineMeta({
		title: 'Monitor/KIA animation',
		component: KiaAnimationStory,
		parameters: { layout: 'fullscreen' },
		args: { durationMs: 2000 }
	});
</script>

<Story name="Default" />
<Story name="Quick preview" args={{ durationMs: 700 }} />
<Story name="Slow motion" args={{ durationMs: 5000 }} />
