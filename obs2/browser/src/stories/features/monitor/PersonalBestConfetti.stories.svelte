<script module lang="ts">
	import { defineMeta } from '@storybook/addon-svelte-csf';
	import PersonalBestConfettiStory from './PersonalBestConfettiStory.svelte';

	const { Story } = defineMeta({
		title: 'Monitor/Personal best confetti',
		component: PersonalBestConfettiStory,
		parameters: { layout: 'fullscreen' }
	});
</script>

<Story name="Mission glass" />
<Story name="Signal band" args={{ design: 'signal-band' }} />
<Story name="For Your Eyes Only" args={{ design: 'debug' }} />
<Story name="Idle until triggered" args={{ autoplay: false }} />
<Story name="Quick preview" args={{ durationMs: 1000 }} />
<Story name="Slow motion" args={{ durationMs: 8000 }} />

<Story name="Reduced motion" args={{ reducedMotion: true }} />
