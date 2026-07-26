<script module lang="ts">
	export interface SegmentedControlOption {
		value: string;
		label: string;
	}
</script>

<script lang="ts">
	let {
		value = $bindable(),
		options,
		ariaLabel,
		onChange
	}: {
		value: string;
		options: SegmentedControlOption[];
		ariaLabel: string;
		onChange?: (value: string) => void;
	} = $props();

	let group = $state<HTMLDivElement>();

	function choose(index: number) {
		const option = options[index];
		if (!option) return;
		value = option.value;
		onChange?.(option.value);
		group?.querySelectorAll<HTMLButtonElement>('[role="radio"]')[index]?.focus();
	}

	function onKeydown(event: KeyboardEvent, index: number) {
		let next: number | undefined;
		if (event.key === 'ArrowLeft' || event.key === 'ArrowUp') {
			next = (index - 1 + options.length) % options.length;
		} else if (event.key === 'ArrowRight' || event.key === 'ArrowDown') {
			next = (index + 1) % options.length;
		} else if (event.key === 'Home') {
			next = 0;
		} else if (event.key === 'End') {
			next = options.length - 1;
		}
		if (next == null) return;
		event.preventDefault();
		choose(next);
	}
</script>

<div
	bind:this={group}
	class="inline-flex overflow-hidden rounded-sm border border-(--obs-border)"
	role="radiogroup"
	aria-label={ariaLabel}
>
	{#each options as option, index}
		<button
			type="button"
			role="radio"
			aria-checked={value === option.value}
			tabindex={value === option.value || (!options.some((candidate) => candidate.value === value) && index === 0)
				? 0
				: -1}
			class="px-3 py-1.5 text-xs font-semibold transition-colors hover:bg-(--obs-control-hover) focus-visible:z-10"
			class:border-l={index > 0}
			class:border-(--obs-border)={index > 0}
			class:bg-(--obs-control-active)={value === option.value}
			onclick={() => choose(index)}
			onkeydown={(event) => onKeydown(event, index)}
		>
			{option.label}
		</button>
	{/each}
</div>
