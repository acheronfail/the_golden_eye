<script module lang="ts">
	export type SettingsSection = 'general' | 'recording' | 'notifications' | 'youtube';
</script>

<script lang="ts">
	let {
		value,
		includeYouTube = false,
		onChange
	}: {
		value: SettingsSection;
		includeYouTube?: boolean;
		onChange: (value: SettingsSection) => void;
	} = $props();

	const sections = $derived<SettingsSection[]>([
		'general',
		'recording',
		'notifications',
		...(includeYouTube ? (['youtube'] as const) : [])
	]);
	const labels: Record<SettingsSection, string> = {
		general: 'General',
		recording: 'Recording',
		notifications: 'Notifications',
		youtube: 'YouTube'
	};
	let group = $state<HTMLDivElement>();

	function choose(index: number) {
		const section = sections[index];
		if (!section) return;
		onChange(section);
		group?.querySelectorAll<HTMLButtonElement>('[role="radio"]')[index]?.focus();
	}

	function onKeydown(event: KeyboardEvent, index: number) {
		let next: number | undefined;
		if (event.key === 'ArrowLeft' || event.key === 'ArrowUp') next = (index - 1 + sections.length) % sections.length;
		else if (event.key === 'ArrowRight' || event.key === 'ArrowDown') next = (index + 1) % sections.length;
		else if (event.key === 'Home') next = 0;
		else if (event.key === 'End') next = sections.length - 1;
		if (next == null) return;
		event.preventDefault();
		choose(next);
	}
</script>

<div
	bind:this={group}
	class="flex w-full overflow-hidden rounded border border-(--obs-border)"
	role="radiogroup"
	aria-label="Settings section"
>
	{#each sections as section, index}
		<button
			type="button"
			role="radio"
			aria-label={labels[section]}
			aria-checked={value === section}
			tabindex={value === section ? 0 : -1}
			title={labels[section]}
			class="flex min-w-0 items-center justify-center gap-2 px-3 py-2.5 text-sm font-semibold transition-colors hover:bg-(--obs-control-hover) focus-visible:z-10 focus-visible:outline-2 focus-visible:outline-offset-[-2px] focus-visible:outline-(--obs-accent)"
			class:w-12={value !== section}
			class:flex-1={value === section}
			class:border-l={index > 0}
			class:border-(--obs-border)={index > 0}
			class:bg-(--obs-control-active)={value === section}
			onclick={() => choose(index)}
			onkeydown={(event) => onKeydown(event, index)}
		>
			<svg
				class="size-4 shrink-0"
				viewBox="0 0 24 24"
				fill="none"
				stroke="currentColor"
				stroke-width="1.8"
				stroke-linecap="round"
				stroke-linejoin="round"
				aria-hidden="true"
			>
				{#if section === 'general'}
					<path d="M4 7h16M4 12h16M4 17h16" />
					<circle cx="9" cy="7" r="2" fill="var(--obs-control-active)" />
					<circle cx="15" cy="12" r="2" fill="var(--obs-control-active)" />
					<circle cx="10" cy="17" r="2" fill="var(--obs-control-active)" />
				{:else if section === 'recording'}
					<circle cx="12" cy="12" r="8" />
					<circle cx="12" cy="12" r="3.5" fill="currentColor" stroke="none" />
				{:else if section === 'notifications'}
					<path d="M18 8a6 6 0 0 0-12 0c0 7-3 7-3 9h18c0-2-3-2-3-9ZM10 21h4" />
				{:else}
					<rect x="3" y="5" width="18" height="14" rx="3" />
					<path d="m10 9 5 3-5 3V9Z" fill="currentColor" stroke="none" />
				{/if}
			</svg>
			{#if value === section}<span aria-hidden="true" class="truncate">{labels[section]}</span>{/if}
		</button>
	{/each}
</div>
