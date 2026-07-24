<script module lang="ts">
	import { defineMeta } from '@storybook/addon-svelte-csf';
	import DateRangeSelect from '$lib/components/DateRangeSelect.svelte';
	const { Story } = defineMeta({ title: 'Statistics/Date range', component: DateRangeSelect });
</script>

<Story
	name="Last 30 days"
	args={{ value: { preset: '30d', customFrom: '2026-07-01', customTo: '2026-07-24' }, bucket: 'week' }}
/>
<Story
	name="Custom"
	args={{ value: { preset: 'custom', customFrom: '2026-07-01', customTo: '2026-07-24' }, bucket: 'day' }}
/>
<Story
	name="Invalid custom"
	args={{
		value: { preset: 'custom', customFrom: '2026-07-24', customTo: '2026-07-01' },
		bucket: 'week',
		error: 'Start date must not be after end date.'
	}}
/>
<Story
	name="Without group by"
	args={{
		value: { preset: '30d', customFrom: '2026-07-01', customTo: '2026-07-24' },
		bucket: 'year',
		showGroupBy: false
	}}
/>
