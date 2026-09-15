<script lang="ts">
	let {
		imageData,
		streaming = false,
		close
	}: {
		imageData: string;
		streaming?: boolean;
		close: () => void;
	} = $props();

	const downloadScreenshot = () => {
		const link = document.createElement('a');
		link.href = imageData;
		link.download = 'screenshot';
		link.click();
	};
</script>

<div class="flex w-full flex-col gap-4 rounded obs-panel p-4">
	<div class="flex flex-row items-center gap-2">
		<h2 class="text-xl font-semibold">Screenshot:</h2>
		{#if !streaming}
			<button class="obs-button obs-button-danger px-2 py-1 font-mono text-sm" onclick={close}>Close</button>
		{/if}
	</div>

	<button class="obs-button self-start obs-button-gold px-2 py-1 font-mono text-sm" onclick={downloadScreenshot}
		>Download screenshot</button
	>

	<img src={imageData} alt="OBS Screenshot" class="max-w-full rounded obs-preview" />
</div>
