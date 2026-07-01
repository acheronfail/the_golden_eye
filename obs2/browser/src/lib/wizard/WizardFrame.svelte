<script lang="ts">
	import { goto } from '$app/navigation';
	import StepBar from './StepBar.svelte';
	import type { Snippet } from 'svelte';

	let {
		step,
		title,
		subtitle,
		hrefs,
		children
	}: {
		/** 1-based index into the StepBar's steps. */
		step: number;
		title: string;
		subtitle?: string;
		/** Destinations for completed steps, forwarded to the StepBar. */
		hrefs?: (string | undefined)[];
		children: Snippet;
	} = $props();

	// Backspace steps back to the previous wizard page (same destination the
	// breadcrumb links to), unless focus is in a field where backspace edits text.
	const onkeydown = (event: KeyboardEvent) => {
		if (event.key !== 'Backspace') return;

		const target = event.target as HTMLElement | null;
		if (target && (target.isContentEditable || ['INPUT', 'TEXTAREA', 'SELECT'].includes(target.tagName))) return;

		const back = hrefs?.[step - 2];
		if (back) {
			event.preventDefault();
			goto(back);
		}
	};
</script>

<svelte:window {onkeydown} />

<main class="mx-auto w-full max-w-xl px-6 py-12">
	<StepBar current={step} {hrefs} />

	<h1 class="text-2xl font-semibold text-amber-300">{title}</h1>
	{#if subtitle}
		<p class="mt-2 mb-8 text-sm text-neutral-400">{subtitle}</p>
	{:else}
		<div class="mb-8"></div>
	{/if}

	{@render children()}
</main>
