<script lang="ts">
	let {
		src,
		alt,
		missing = false,
		markMissing = () => {},
		markAvailable = () => {}
	}: {
		src: string;
		alt: string;
		missing?: boolean;
		markMissing?: () => void;
		markAvailable?: () => void;
	} = $props();
</script>

{#if missing}
	<div class="obs-preview-missing aspect-video max-h-36 w-full shrink-0 sm:h-36 sm:w-64">
		<span class="px-3 font-mono text-xs leading-snug">No image returned from OBS</span>
		<img {src} alt="" aria-hidden="true" class="hidden" onload={markAvailable} />
	</div>
{:else}
	<img
		{src}
		{alt}
		loading="lazy"
		onerror={markMissing}
		class="aspect-video max-h-36 w-full shrink-0 obs-preview object-contain sm:h-36 sm:w-auto"
	/>
{/if}
