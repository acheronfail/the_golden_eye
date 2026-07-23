<script module lang="ts">
	import { defineMeta } from '@storybook/addon-svelte-csf';
	import ModalStory from './ModalStory.svelte';

	const { Story } = defineMeta({
		title: 'App/Other modals',
		component: ModalStory,
		parameters: { layout: 'fullscreen' },
		args: { kind: 'welcome' }
	});
</script>

<Story name="Welcome" />
<Story name="Reset settings" args={{ kind: 'reset' }} />
<Story name="Resetting settings" args={{ kind: 'reset', busy: true }} />
<Story name="Reset settings error" args={{ kind: 'reset', error: 'The settings file could not be removed.' }} />
