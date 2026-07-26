<script lang="ts" module>
	export interface SelectOption {
		/** The value bound back to the caller. */
		value: string;
		/** Human-readable label shown in the trigger and the option list. */
		label: string;
		/** When true, the option is shown greyed out and cannot be chosen. */
		disabled?: boolean;
	}
</script>

<script lang="ts">
	import { tick } from 'svelte';

	// Why this exists: OBS's browser dock uses CEF, and on Linux CEF renders the
	// native <select> popup as a separate top-level OS window. OBS doesn't keep
	// that window focused, so the menu flashes open and is dismissed immediately
	// -- selects are effectively unusable in the dock. This component renders the
	// option list as ordinary in-page DOM (portaled to <body> so container
	// overflow can't clip it), which stays inside the CEF surface and works
	// everywhere. It follows the ARIA combobox+listbox pattern so the browser
	// tests and assistive tech still see it as a <select>-equivalent.

	let {
		value = $bindable(),
		options,
		onChange,
		id,
		class: className = '',
		disabled = false,
		placeholder = 'select'
	}: {
		/** Currently selected value. Supports `bind:value`. */
		value?: string;
		options: SelectOption[];
		/** Called with the newly chosen value on every change. */
		onChange?: (value: string) => void;
		/** Applied to the trigger; forwarded to `<label for>` associations. */
		id?: string;
		/** Extra classes for the trigger button (on top of `obs-select`). */
		class?: string;
		disabled?: boolean;
		/** Trigger text shown when no option matches the current value. */
		placeholder?: string;
	} = $props();

	const uid = $props.id();
	const listId = `${uid}-list`;
	const optionId = (index: number) => `${uid}-opt-${index}`;

	let open = $state(false);
	let activeIndex = $state(-1);
	let triggerEl = $state<HTMLButtonElement>();
	let listEl = $state<HTMLUListElement>();
	let menuPosition = $state({
		left: '0px',
		minWidth: '0px',
		maxHeight: '120px',
		top: 'auto',
		bottom: 'auto'
	});
	let typeahead = '';
	let typeaheadTimer: ReturnType<typeof setTimeout> | undefined;

	const selected = $derived(options.find((option) => option.value === value));

	function firstEnabledIndex(from = 0, step = 1): number {
		for (let i = from; i >= 0 && i < options.length; i += step) {
			if (!options[i].disabled) return i;
		}
		return -1;
	}

	function positionMenu() {
		if (!triggerEl || !listEl) return;
		const rect = triggerEl.getBoundingClientRect();
		const margin = 4;
		const spaceBelow = window.innerHeight - rect.bottom - margin;
		const spaceAbove = rect.top - margin;
		// Flip above only when there's clearly more room there.
		const above = spaceBelow < 160 && spaceAbove > spaceBelow;
		const maxHeight = Math.max(120, Math.floor(above ? spaceAbove : spaceBelow));
		menuPosition = {
			left: `${Math.round(rect.left)}px`,
			minWidth: `${Math.round(rect.width)}px`,
			maxHeight: `${maxHeight}px`,
			top: above ? 'auto' : `${Math.round(rect.bottom + margin)}px`,
			bottom: above ? `${Math.round(window.innerHeight - rect.top + margin)}px` : 'auto'
		};
	}

	function scrollActiveIntoView() {
		if (activeIndex < 0) return;
		const el = listEl?.querySelector(`#${CSS.escape(optionId(activeIndex))}`);
		el?.scrollIntoView?.({ block: 'nearest' });
	}

	async function openMenu() {
		if (disabled || open) return;
		open = true;
		const current = options.findIndex((option) => option.value === value);
		activeIndex = current >= 0 && !options[current].disabled ? current : firstEnabledIndex();
		await tick();
		positionMenu();
		scrollActiveIntoView();
	}

	function closeMenu(refocus = true) {
		if (!open) return;
		open = false;
		activeIndex = -1;
		if (refocus) triggerEl?.focus();
	}

	function choose(index: number) {
		const option = options[index];
		if (!option || option.disabled) return;
		if (option.value !== value) {
			value = option.value;
			onChange?.(option.value);
		}
		closeMenu();
	}

	function moveActive(step: number) {
		if (options.length === 0) return;
		let next = activeIndex;
		for (let i = 0; i < options.length; i++) {
			next = (next + step + options.length) % options.length;
			if (!options[next].disabled) break;
		}
		activeIndex = next;
		scrollActiveIntoView();
	}

	function applyTypeahead(char: string) {
		clearTimeout(typeaheadTimer);
		typeahead += char.toLowerCase();
		typeaheadTimer = setTimeout(() => (typeahead = ''), 500);
		const match = options.findIndex((option) => !option.disabled && option.label.toLowerCase().startsWith(typeahead));
		if (match >= 0) {
			activeIndex = match;
			scrollActiveIntoView();
		}
	}

	function onTriggerKeydown(event: KeyboardEvent) {
		switch (event.key) {
			case 'ArrowDown':
			case 'ArrowUp':
				event.preventDefault();
				if (!open) {
					openMenu();
				} else {
					moveActive(event.key === 'ArrowDown' ? 1 : -1);
				}
				return;
			case 'Home':
				if (open) {
					event.preventDefault();
					activeIndex = firstEnabledIndex();
					scrollActiveIntoView();
				}
				return;
			case 'End':
				if (open) {
					event.preventDefault();
					activeIndex = firstEnabledIndex(options.length - 1, -1);
					scrollActiveIntoView();
				}
				return;
			case 'Enter':
			case ' ':
				event.preventDefault();
				if (open) choose(activeIndex);
				else openMenu();
				return;
			case 'Escape':
				if (open) {
					event.preventDefault();
					closeMenu();
				}
				return;
			case 'Tab':
				closeMenu(false);
				return;
		}
		if (event.key.length === 1 && !event.ctrlKey && !event.metaKey && !event.altKey) {
			if (!open) openMenu();
			applyTypeahead(event.key);
		}
	}

	// Portal the menu to <body> so scroll containers / stacking contexts can't
	// clip it. Reposition on scroll or resize; closing on those would be jarring
	// while the trigger is still on screen, so we just track it instead.
	function portal(node: HTMLElement) {
		document.body.appendChild(node);
		return {
			destroy() {
				node.remove();
			}
		};
	}

	$effect(() => {
		if (!open) return;
		const reposition = () => positionMenu();
		window.addEventListener('scroll', reposition, true);
		window.addEventListener('resize', reposition);
		return () => {
			window.removeEventListener('scroll', reposition, true);
			window.removeEventListener('resize', reposition);
		};
	});
</script>

<button
	bind:this={triggerEl}
	{id}
	type="button"
	{disabled}
	role="combobox"
	aria-controls={listId}
	aria-expanded={open}
	aria-haspopup="listbox"
	aria-activedescendant={open && activeIndex >= 0 ? optionId(activeIndex) : undefined}
	class="flex obs-select items-center text-left disabled:cursor-not-allowed {className}"
	class:obs-select-placeholder={!selected}
	onclick={() => (open ? closeMenu() : openMenu())}
	onkeydown={onTriggerKeydown}
>
	<span class="min-w-0 flex-1 truncate">{selected ? selected.label : placeholder}</span>
</button>

{#if open}
	<!-- Backdrop swallows outside clicks. Transparent, full-viewport. -->
	<button
		type="button"
		tabindex="-1"
		aria-hidden="true"
		class="fixed inset-0 z-40 cursor-default"
		onclick={() => closeMenu()}
		use:portal
	></button>
	<ul
		bind:this={listEl}
		id={listId}
		role="listbox"
		tabindex="-1"
		style:--select-left={menuPosition.left}
		style:--select-min-width={menuPosition.minWidth}
		style:--select-max-height={menuPosition.maxHeight}
		style:--select-top={menuPosition.top}
		style:--select-bottom={menuPosition.bottom}
		class="fixed top-(--select-top) bottom-(--select-bottom) left-(--select-left) z-50 obs-select-menu max-h-(--select-max-height) min-w-(--select-min-width) overflow-auto rounded"
		use:portal
	>
		{#each options as option, index (option.value)}
			<!-- Keyboard handling lives on the combobox trigger (ARIA listbox pattern),
			     so the option rows only need pointer handlers. -->
			<!-- svelte-ignore a11y_click_events_have_key_events -->
			<li
				id={optionId(index)}
				role="option"
				aria-selected={option.value === value}
				aria-disabled={option.disabled}
				class="obs-select-option truncate px-3 py-1.5 text-sm"
				class:obs-select-option-active={index === activeIndex}
				class:obs-select-option-disabled={option.disabled}
				onclick={() => choose(index)}
				onmousemove={() => {
					if (!option.disabled) activeIndex = index;
				}}
			>
				{option.label}
			</li>
		{/each}
	</ul>
{/if}
