<script lang="ts">
	import Tooltip from '$lib/ui/Tooltip.svelte';
</script>

<main class="grid min-h-64 place-items-center bg-(--obs-bg) p-8 text-(--obs-text)">
	<Tooltip
		content="Appears immediately on pointer hover or keyboard focus, and stays within the visible browser dock."
		class="cursor-help"
	>
		<span class="border-b border-dotted border-(--obs-text-dim) font-mono text-xs tracking-wider uppercase">
			Time in level
		</span>
	</Tooltip>
</main>
