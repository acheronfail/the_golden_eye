<script module lang="ts">
	import { defineMeta } from '@storybook/addon-svelte-csf';
	import MonitorPersonalBestStory from './MonitorPersonalBestStory.svelte';

	const { Story } = defineMeta({
		title: 'Monitor/Personal best',
		component: MonitorPersonalBestStory,
		parameters: { layout: 'fullscreen' }
	});
</script>

<Story name="Mission glass: Waiting" args={{ design: 'mission-glass', scenario: 'waiting' }} />
<Story name="Mission glass: Start with PB" args={{ design: 'mission-glass', scenario: 'start' }} />
<Story name="Mission glass: Start without PB" args={{ design: 'mission-glass', scenario: 'startMissing' }} />
<Story name="Mission glass: Playing with PB" args={{ design: 'mission-glass', scenario: 'playing' }} />
<Story name="Mission glass: Playing without PB" args={{ design: 'mission-glass', scenario: 'playingMissing' }} />
<Story name="Mission glass: Complete before stats" args={{ design: 'mission-glass', scenario: 'complete' }} />
<Story name="Mission glass: Failed before stats" args={{ design: 'mission-glass', scenario: 'failed' }} />
<Story name="Mission glass: Aborted before stats" args={{ design: 'mission-glass', scenario: 'aborted' }} />
<Story name="Mission glass: KIA before stats" args={{ design: 'mission-glass', scenario: 'kia' }} />
<Story name="Mission glass: Stats" args={{ design: 'mission-glass', scenario: 'stats' }} />
<Story name="Mission glass: Cancelled" args={{ design: 'mission-glass', scenario: 'cancelled' }} />
<Story name="Mission glass: Skipped stats" args={{ design: 'mission-glass', scenario: 'statsSkipped' }} />
<Story name="Mission glass: Back to selection" args={{ design: 'mission-glass', scenario: 'selection' }} />
<Story name="Signal band: Waiting" args={{ design: 'signal-band', scenario: 'waiting' }} />
<Story name="Signal band: Start with PB" args={{ design: 'signal-band', scenario: 'start' }} />
<Story name="Signal band: Start without PB" args={{ design: 'signal-band', scenario: 'startMissing' }} />
<Story name="Signal band: Playing with PB" args={{ design: 'signal-band', scenario: 'playing' }} />
<Story name="Signal band: Playing without PB" args={{ design: 'signal-band', scenario: 'playingMissing' }} />
<Story name="Signal band: Complete before stats" args={{ design: 'signal-band', scenario: 'complete' }} />
<Story name="Signal band: Failed before stats" args={{ design: 'signal-band', scenario: 'failed' }} />
<Story name="Signal band: Aborted before stats" args={{ design: 'signal-band', scenario: 'aborted' }} />
<Story name="Signal band: KIA before stats" args={{ design: 'signal-band', scenario: 'kia' }} />
<Story name="Signal band: Stats" args={{ design: 'signal-band', scenario: 'stats' }} />
<Story name="Signal band: Cancelled" args={{ design: 'signal-band', scenario: 'cancelled' }} />
<Story name="Signal band: Skipped stats" args={{ design: 'signal-band', scenario: 'statsSkipped' }} />
<Story name="Signal band: Back to selection" args={{ design: 'signal-band', scenario: 'selection' }} />
<Story name="For Your Eyes Only: Waiting" args={{ design: 'debug', scenario: 'waiting' }} />
<Story name="For Your Eyes Only: Start with PB" args={{ design: 'debug', scenario: 'start' }} />
<Story name="For Your Eyes Only: Start without PB" args={{ design: 'debug', scenario: 'startMissing' }} />
<Story name="For Your Eyes Only: Playing with PB" args={{ design: 'debug', scenario: 'playing' }} />
<Story name="For Your Eyes Only: Playing without PB" args={{ design: 'debug', scenario: 'playingMissing' }} />
<Story name="For Your Eyes Only: Complete before stats" args={{ design: 'debug', scenario: 'complete' }} />
<Story name="For Your Eyes Only: Failed before stats" args={{ design: 'debug', scenario: 'failed' }} />
<Story name="For Your Eyes Only: Aborted before stats" args={{ design: 'debug', scenario: 'aborted' }} />
<Story name="For Your Eyes Only: KIA before stats" args={{ design: 'debug', scenario: 'kia' }} />
<Story name="For Your Eyes Only: Stats" args={{ design: 'debug', scenario: 'stats' }} />
<Story name="For Your Eyes Only: Cancelled" args={{ design: 'debug', scenario: 'cancelled' }} />
<Story name="For Your Eyes Only: Skipped stats" args={{ design: 'debug', scenario: 'statsSkipped' }} />
<Story name="For Your Eyes Only: Back to selection" args={{ design: 'debug', scenario: 'selection' }} />
