<script module lang="ts">
	import { defineMeta } from '@storybook/addon-svelte-csf';
	import ModalStory from './ModalStory.svelte';

	const { Story } = defineMeta({
		title: 'App/Other modals',
		component: ModalStory,
		parameters: { layout: 'fullscreen' },
		args: { kind: 'modal-dialog' }
	});
</script>

<Story name="Shared modal dialog" />
<Story name="Replay buffer preference" args={{ kind: 'replay-buffer' }} />
<Story name="Saving replay buffer preference" args={{ kind: 'replay-buffer', busy: true }} />
<Story
	name="Replay buffer preference error"
	args={{ kind: 'replay-buffer', error: 'The preference could not be saved.' }}
/>
<Story name="Welcome" args={{ kind: 'welcome' }} />
<Story name="Reset settings" args={{ kind: 'reset' }} />
<Story name="Resetting settings" args={{ kind: 'reset', busy: true }} />
<Story name="Reset settings error" args={{ kind: 'reset', error: 'The settings file could not be removed.' }} />
<Story name="Building runs library" args={{ kind: 'catalog-initial' }} />
<Story name="Resyncing runs library" args={{ kind: 'catalog-manual' }} />
