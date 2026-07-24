<script module lang="ts">
	import { defineMeta } from '@storybook/addon-svelte-csf';
	import Chart from '$lib/components/Chart.svelte';

	const { Story } = defineMeta({
		title: 'Statistics/Chart',
		component: Chart,
		args: {
			title: 'Run times',
			description: 'Example responsive statistics chart',
			xLabel: 'Date',
			yLabel: 'Time',
			data: {
				kind: 'line',
				xType: 'time',
				series: [
					{
						id: 'complete',
						label: 'Complete',
						color: 'var(--obs-success)',
						surfaceColor: 'var(--obs-success-surface)',
						points: [
							{ x: Date.UTC(2026, 6, 1), y: 82 },
							{ x: Date.UTC(2026, 6, 8), y: 74 }
						]
					}
				]
			}
		}
	});
</script>

<Story name="Line" />
<Story
	name="One point"
	args={{
		data: {
			kind: 'line',
			xType: 'time',
			series: [{ id: 'one', label: 'One', color: 'var(--obs-gold)', points: [{ x: Date.UTC(2026, 6, 1), y: 72 }] }]
		}
	}}
/>
<Story name="Empty" args={{ data: { kind: 'groupedBar', xType: 'category', series: [] } }} />
<Story
	name="Grouped bars"
	args={{
		data: {
			kind: 'groupedBar',
			xType: 'category',
			series: [
				{
					id: 'failed',
					label: 'Failed',
					color: 'var(--obs-danger)',
					surfaceColor: 'var(--obs-danger-surface)',
					points: [
						{ x: 'Week 1', y: 7 },
						{ x: 'Week 2', y: 4 }
					]
				},
				{
					id: 'abort',
					label: 'Aborted',
					color: 'var(--obs-danger)',
					surfaceColor: 'var(--obs-danger-surface)',
					pattern: 'diagonal',
					points: [
						{ x: 'Week 1', y: 9 },
						{ x: 'Week 2', y: 5 }
					]
				}
			]
		}
	}}
/>
<Story
	name="Edge-aligned time bars"
	args={{
		data: {
			kind: 'stackedBar',
			xType: 'time',
			series: [
				{
					id: 'complete',
					label: 'Complete',
					color: 'var(--obs-success)',
					surfaceColor: 'var(--obs-success-surface)',
					points: [
						{ x: Date.UTC(2026, 6, 13), y: 1 },
						{ x: Date.UTC(2026, 6, 20), y: 12 }
					]
				},
				{
					id: 'kia',
					label: 'Killed in Action',
					color: 'var(--obs-danger)',
					surfaceColor: 'var(--obs-danger-surface)',
					pattern: 'dots',
					points: [
						{ x: Date.UTC(2026, 6, 13), y: 1 },
						{ x: Date.UTC(2026, 6, 20), y: 5 }
					]
				}
			]
		}
	}}
/>
<Story
	name="Clustered time labels"
	args={{
		data: {
			kind: 'line',
			xType: 'time',
			series: [
				{
					id: 'complete',
					label: 'Complete',
					color: 'var(--obs-success)',
					points: [
						{ x: Date.UTC(2026, 6, 15), y: 28 },
						{ x: Date.UTC(2026, 6, 22, 8), y: 28 },
						{ x: Date.UTC(2026, 6, 22, 16), y: 28 },
						{ x: Date.UTC(2026, 6, 23), y: 28 }
					]
				}
			]
		}
	}}
/>
