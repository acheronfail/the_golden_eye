<script lang="ts">
	import { goto } from '$app/navigation';
	import WizardFrame from '$lib/wizard/WizardFrame.svelte';
	import OptionList, { type Option } from '$lib/wizard/OptionList.svelte';
	import type { PageProps } from './$types';

	let { params }: PageProps = $props();

	const options: (Option & { lang: 'en' | 'jp' })[] = [
		{
			title: 'English',
			detail: 'PAL (AU, Europe, etc) or NTSC U (North America, etc)',
			lang: 'en'
		},
		{
			title: 'Japanese',
			detail: 'NTSC J',
			lang: 'jp'
		}
	];

	const select = (_: Option, index: number) => {
		const lang = options[index].lang;
		goto(`/source/${encodeURIComponent(params.sourceName)}/${lang}`);
	};
</script>

<svelte:head>
	<title>Setup | {params.sourceName}</title>
</svelte:head>

<WizardFrame step={2} title="Which version are you playing?" subtitle="Source: {params.sourceName}" hrefs={['/']}>
	<OptionList {options} onSelect={select} />
</WizardFrame>
