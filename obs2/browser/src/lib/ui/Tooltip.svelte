<script lang="ts">
	import { tick, type Snippet } from 'svelte';

	let {
		content,
		children,
		class: className = ''
	}: {
		content: string;
		children: Snippet;
		class?: string;
	} = $props();

	const uid = $props.id();
	const tooltipId = `${uid}-tooltip`;
	let trigger = $state<HTMLButtonElement>();
	let tooltip = $state<HTMLSpanElement>();
	let open = $state(false);
	let positioned = $state(false);
	let position = $state({ left: 0, top: 0 });

	function portal(node: HTMLElement) {
		document.body.appendChild(node);
		return {
			destroy() {
				node.remove();
			}
		};
	}

	function positionTooltip() {
		if (!open || !trigger || !tooltip) return;
		const triggerRect = trigger.getBoundingClientRect();
		const width = tooltip.offsetWidth;
		const height = tooltip.offsetHeight;
		const margin = 8;
		const gap = 7;
		const centeredLeft = triggerRect.left + triggerRect.width / 2 - width / 2;
		const left = Math.max(margin, Math.min(window.innerWidth - width - margin, centeredLeft));
		const fitsAbove = triggerRect.top - height - gap >= margin;
		const preferredTop = fitsAbove ? triggerRect.top - height - gap : triggerRect.bottom + gap;
		const top = Math.max(margin, Math.min(window.innerHeight - height - margin, preferredTop));
		position = { left: Math.round(left), top: Math.round(top) };
		positioned = true;
	}

	async function show() {
		open = true;
		positioned = false;
		await tick();
		positionTooltip();
	}

	function hide() {
		open = false;
		positioned = false;
	}

	function closeOnEscape(event: KeyboardEvent) {
		if (event.key === 'Escape') hide();
	}

	$effect(() => {
		if (!open) return;
		const reposition = () => positionTooltip();
		window.addEventListener('scroll', reposition, true);
		window.addEventListener('resize', reposition);
		return () => {
			window.removeEventListener('scroll', reposition, true);
			window.removeEventListener('resize', reposition);
		};
	});
</script>

<button
	bind:this={trigger}
	type="button"
	class="font-inherit inline-flex appearance-none border-0 bg-transparent p-0 text-left text-inherit focus-visible:rounded-sm focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-(--obs-gold-hover) {className}"
	aria-describedby={open ? tooltipId : undefined}
	onpointerenter={show}
	onpointerleave={hide}
	onfocus={show}
	onblur={hide}
	onkeydown={closeOnEscape}
>
	{@render children()}
</button>

{#if open}
	<span
		bind:this={tooltip}
		id={tooltipId}
		role="tooltip"
		style={`left: ${position.left}px; top: ${position.top}px`}
		class="pointer-events-none fixed z-60 max-w-[min(22rem,calc(100vw-1rem))] rounded border border-(--obs-border-soft) bg-(--obs-panel) px-3 py-2 font-sans text-xs leading-relaxed font-normal tracking-normal text-(--obs-text-muted) normal-case shadow-[0_10px_28px_rgb(0_0_0_/_48%),inset_0_1px_0_var(--obs-border-soft)]"
		class:invisible={!positioned}
		use:portal
	>
		{content}
	</span>
{/if}
