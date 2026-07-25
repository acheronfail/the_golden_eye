<script module lang="ts">
	import { defineMeta } from '@storybook/addon-svelte-csf';
	import ActionMenuStory from './ActionMenuStory.svelte';

	const { Story } = defineMeta({
		title: 'Runs/Actions menu',
		component: ActionMenuStory,
		parameters: { layout: 'centered' }
	});
</script>

<Story name="Default" />
<Story name="Danger action" args={{ danger: true }} />
