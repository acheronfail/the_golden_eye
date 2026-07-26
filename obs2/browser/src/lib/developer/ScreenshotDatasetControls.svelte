<script lang="ts">
	let {
		imageData,
		language,
		streaming = false,
		close
	}: {
		imageData: string;
		language: 'en' | 'jp';
		streaming?: boolean;
		close: () => void;
	} = $props();

	let statsScreenIndex = $state(0);
	let startScreenIndex = $state(0);
	let failedScreenIndex = $state(0);

	const startScreenNames = $derived.by(() => {
		const values: string[] = [];
		for (let level = 1; level <= 20; level++) {
			for (const difficulty of ['Agent', 'Secret Agent', '00 Agent']) {
				values.push(`${language} - start - ${level} - ${difficulty}`);
			}
		}
		return values;
	});
	const failedScreenNames = $derived.by(() => {
		const values: string[] = [];
		for (let level = 1; level <= 20; level++) {
			for (const difficulty of ['Agent', 'Secret Agent', '00 Agent']) {
				for (const status of ['complete', 'failed', 'abort', 'kia']) {
					values.push(`${language} - ${status} - ${level} - ${difficulty}`);
				}
			}
		}
		return values;
	});
	const statsScreenNames = $derived.by(() => {
		const values: string[] = [];
		for (let level = 1; level <= 20; level++) {
			for (const difficulty of ['Agent', 'Secret Agent', '00 Agent']) {
				values.push(`${language} - stats - ${level} - ${difficulty} - TIMES_HERE`);
			}
		}
		return values;
	});

	const saveScreenshot = (names: string[], index: number): number => {
		const link = document.createElement('a');
		link.href = imageData;
		link.download = `${names[index]}.bmp`;
		link.click();
		return (index + 1) % names.length;
	};
</script>

<div class="flex w-full flex-col gap-4 rounded obs-panel p-4">
	<div class="flex flex-row items-center gap-2">
		<h2 class="text-xl font-semibold">Screenshot:</h2>
		{#if !streaming}
			<button class="obs-button obs-button-danger px-2 py-1 font-mono text-sm" onclick={close}>Close</button>
		{/if}
	</div>

	<div class="flex flex-row gap-2">
		<button
			class="obs-button px-2 py-1 font-mono text-sm"
			onclick={() => (startScreenIndex = (startScreenIndex - 1 + startScreenNames.length) % startScreenNames.length)}
			>-1</button
		>
		<button
			class="obs-button px-2 py-1 font-mono text-sm"
			onclick={() => (startScreenIndex = (startScreenIndex + 1) % startScreenNames.length)}>+1</button
		>
		<button
			class="obs-button obs-button-gold px-2 py-1 font-mono text-sm"
			onclick={() => (startScreenIndex = saveScreenshot(startScreenNames, startScreenIndex))}
			>save "{startScreenNames[startScreenIndex]}.bmp"</button
		>
	</div>

	<div class="flex flex-row gap-2">
		<button
			class="obs-button px-2 py-1 font-mono text-sm"
			onclick={() =>
				(failedScreenIndex = (failedScreenIndex - 1 + failedScreenNames.length) % failedScreenNames.length)}>-1</button
		>
		<button
			class="obs-button px-2 py-1 font-mono text-sm"
			onclick={() => (failedScreenIndex = (failedScreenIndex + 1) % failedScreenNames.length)}>+1</button
		>
		<button
			class="obs-button obs-button-gold px-2 py-1 font-mono text-sm"
			onclick={() => (failedScreenIndex = saveScreenshot(failedScreenNames, failedScreenIndex))}
			>save "{failedScreenNames[failedScreenIndex]}.bmp"</button
		>
	</div>

	<div class="flex flex-row gap-2">
		<button
			class="obs-button px-2 py-1 font-mono text-sm"
			onclick={() => (statsScreenIndex = (statsScreenIndex - 1 + statsScreenNames.length) % statsScreenNames.length)}
			>-1</button
		>
		<button
			class="obs-button px-2 py-1 font-mono text-sm"
			onclick={() => (statsScreenIndex = (statsScreenIndex + 1) % statsScreenNames.length)}>+1</button
		>
		<button
			class="obs-button obs-button-gold px-2 py-1 font-mono text-sm"
			onclick={() => (statsScreenIndex = saveScreenshot(statsScreenNames, statsScreenIndex))}
			>save "{statsScreenNames[statsScreenIndex]}.bmp"</button
		>
	</div>

	<img src={imageData} alt="OBS Screenshot" class="max-w-full rounded obs-preview" />
</div>
