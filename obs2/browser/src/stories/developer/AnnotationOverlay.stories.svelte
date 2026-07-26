<script module lang="ts">
	import { defineMeta } from '@storybook/addon-svelte-csf';
	import AnnotationOverlay from '$lib/developer/AnnotationOverlay.svelte';

	const preview =
		'data:image/svg+xml,%3Csvg xmlns="http://www.w3.org/2000/svg" width="640" height="360"%3E%3Crect width="640" height="360" fill="%23101620"/%3E%3C/svg%3E';

	const { Story } = defineMeta({
		title: 'Developer/Annotation Overlay',
		component: AnnotationOverlay,
		args: {
			imageData: preview,
			frameWidth: 640,
			frameHeight: 360,
			selectedAnnotationSetId: 'time_digits',
			hiddenAnnotationIds: [],
			annotationSets: [
				{
					id: 'time_digits',
					label: 'Time digits',
					annotations: [
						{ label: 'minutes tens', x: 250, y: 140, w: 30, h: 44, score: 0.98 },
						{ label: 'minutes ones', x: 286, y: 140, w: 30, h: 44, score: 0.92 },
						{ label: 'seconds tens', x: 332, y: 140, w: 30, h: 44, score: 0.95 }
					]
				}
			]
		}
	});
</script>

<Story name="Default" />
<Story name="One hidden" args={{ hiddenAnnotationIds: ['time_digits:1'] }} />
