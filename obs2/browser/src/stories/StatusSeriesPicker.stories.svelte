<script module lang="ts">
	import { defineMeta } from '@storybook/addon-svelte-csf';
	import StatusSeriesPicker from '$lib/components/StatusSeriesPicker.svelte';
	const { Story } = defineMeta({ title: 'Statistics/Status picker', component: StatusSeriesPicker });
</script>

<Story name="Abort and failed" args={{ value: ['abort', 'failed'] }} />
<Story name="All statuses" args={{ value: ['complete', 'failed', 'abort', 'kia'] }} />
