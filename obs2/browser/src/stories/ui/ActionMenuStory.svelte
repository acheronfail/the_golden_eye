<script lang="ts">
	import ActionMenu, { type ActionMenuItem } from '$lib/ui/ActionMenu.svelte';

	let { danger = false }: { danger?: boolean } = $props();
	const noop = () => {};
	const runItems: ActionMenuItem[] = [
		{ label: 'Open', action: noop },
		{ label: 'Rename', action: noop },
		{ label: 'Show in Finder', action: noop },
		{ label: 'Keep', action: noop },
		{ label: 'Delete', action: noop, tone: 'danger' }
	];
	const pageItems: ActionMenuItem[] = [
		{ label: 'Show in Finder', action: noop },
		{ label: 'Read clips', action: noop }
	];
</script>

{#if danger}
	<div class="relative">
		<ActionMenu items={runItems} label="More actions" triggerClass="h-8 w-8 px-2 font-mono text-lg" open />
	</div>
{:else}
	<div class="relative flex">
		<button type="button" class="obs-button h-8 rounded-r-none border-r-0 obs-button-gold px-3 font-mono text-xs">
			+ Add times
		</button>
		<ActionMenu
			items={pageItems}
			label="More run actions"
			title="More run actions"
			triggerClass="h-8 w-8 rounded-l-none px-2 font-mono text-sm"
			triggerGlyph="▾"
			open
		/>
	</div>
{/if}
