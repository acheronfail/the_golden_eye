<script module lang="ts">
	import { defineMeta } from '@storybook/addon-svelte-csf';
	import SegmentedControl from '$lib/components/SegmentedControl.svelte';

	const { Story } = defineMeta({
		title: 'Components/Segmented control',
		component: SegmentedControl,
		args: {
			value: 'share',
			ariaLabel: 'Display mode',
			options: [
				{ value: 'share', label: 'Share' },
				{ value: 'count', label: 'Count' }
			]
		}
	});
</script>

<Story name="Two options" />
<Story
	name="Three options"
	args={{
		value: 'week',
		options: [
			{ value: 'day', label: 'Day' },
			{ value: 'week', label: 'Week' },
			{ value: 'month', label: 'Month' }
		]
	}}
/>
