<script module lang="ts">
	export type ActionMenuItem = {
		label: string;
		action: () => void | Promise<void>;
		tone?: 'default' | 'danger';
	};
</script>

<script lang="ts">
	import { tick } from 'svelte';

	let {
		items,
		label = 'More actions',
		title = label,
		busy = false,
		open = $bindable(false),
		onOpenChange,
		triggerClass = ''
	}: {
		items: ActionMenuItem[];
		label?: string;
		title?: string;
		busy?: boolean;
		open?: boolean;
		onOpenChange?: (open: boolean) => void;
		triggerClass?: string;
	} = $props();

	let triggerElement = $state<HTMLButtonElement>();
	let menuElement = $state<HTMLDivElement>();
	let placement = $state<'above' | 'below'>('below');

	function setOpen(nextOpen: boolean) {
		open = nextOpen;
		onOpenChange?.(nextOpen);
	}

	function updatePlacement() {
		if (!open || !triggerElement || !menuElement) return;
		const triggerRect = triggerElement.getBoundingClientRect();
		const viewportPadding = 8;
		const spaceAbove = triggerRect.top - viewportPadding;
		const spaceBelow = window.innerHeight - triggerRect.bottom - viewportPadding;
		placement = spaceBelow < menuElement.offsetHeight && spaceAbove > spaceBelow ? 'above' : 'below';
	}

	async function toggle(event: MouseEvent) {
		event.stopPropagation();
		setOpen(!open);
		if (open) {
			await tick();
			updatePlacement();
		}
	}

	function runAction(item: ActionMenuItem) {
		setOpen(false);
		void item.action();
	}

	function closeOnEscape(event: KeyboardEvent) {
		if (event.key !== 'Escape' || !open) return;
		setOpen(false);
		triggerElement?.focus();
	}

	$effect(() => {
		if (open) void tick().then(updatePlacement);
	});
</script>

<svelte:window onclick={() => open && setOpen(false)} onkeydown={closeOnEscape} onresize={updatePlacement} />

<button
	bind:this={triggerElement}
	type="button"
	class="obs-icon-button {triggerClass}"
	class:obs-icon-button-open={open}
	aria-label={label}
	{title}
	aria-haspopup="menu"
	aria-expanded={open}
	disabled={busy}
	onclick={toggle}
>
	<span aria-hidden="true">⋯</span>
</button>

{#if open}
	<div
		bind:this={menuElement}
		class="obs-menu-panel absolute right-0 z-20 grid max-h-[calc(100vh-1rem)] w-40 overflow-y-auto rounded p-1"
		class:bottom-full={placement === 'above'}
		class:mb-1={placement === 'above'}
		class:top-full={placement === 'below'}
		class:mt-1={placement === 'below'}
		role="menu"
		tabindex="-1"
		aria-label={title}
	>
		{#each items as item}
			<button
				type="button"
				role="menuitem"
				class="obs-menu-link rounded px-3 py-2 text-left font-mono text-xs"
				class:obs-menu-link-danger={item.tone === 'danger'}
				onclick={() => runAction(item)}>{item.label}</button
			>
		{/each}
	</div>
{/if}
