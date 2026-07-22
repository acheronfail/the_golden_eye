<script lang="ts">
	import type { RunClip } from '$lib/api';
	import ActionMenu, { type ActionMenuItem } from '$lib/components/ActionMenu.svelte';
	import MetaPills from '$lib/components/MetaPills.svelte';
	import { formatDate, isCompleted, runMetaChips } from '$lib/utils/runsView';

	let {
		clip,
		busy = false,
		menuOpen = $bindable(false),
		onMenuOpenChange,
		fileBrowserLabel,
		open,
		rename,
		reveal,
		remove
	}: {
		clip: RunClip;
		busy?: boolean;
		menuOpen?: boolean;
		onMenuOpenChange?: (open: boolean) => void;
		fileBrowserLabel: string;
		open: (clip: RunClip) => void;
		rename: (clip: RunClip) => void | Promise<void>;
		reveal: (clip: RunClip) => void | Promise<void>;
		remove: (clip: RunClip) => void | Promise<void>;
	} = $props();

	const actionItems = $derived<ActionMenuItem[]>([
		{ label: 'Open', action: () => open(clip) },
		{ label: 'Rename', action: () => rename(clip) },
		{ label: fileBrowserLabel, action: () => reveal(clip) },
		{ label: 'Delete', action: () => remove(clip), tone: 'danger' }
	]);
</script>

<div class="relative grid grid-cols-[minmax(0,1fr)_auto] gap-1.5">
	<button
		type="button"
		class="obs-list-button group grid min-h-16 min-w-0 grid-cols-[minmax(0,1fr)_auto] items-center gap-2 rounded px-3 py-2 text-left transition-colors"
		class:obs-list-button-success={isCompleted(clip)}
		onclick={() => open(clip)}
	>
		<span class="flex min-w-0 flex-col gap-1">
			<MetaPills chips={runMetaChips(clip)} containerClass="obs-list-title" pillClass="text-[11px]" />
			<span
				class="min-w-0 truncate font-mono text-[10px] text-(--obs-text-muted)"
				title={formatDate(clip.metadata.timestamp)}
			>
				Achieved: {formatDate(clip.metadata.timestamp)}
			</span>
			<span class="obs-list-detail min-w-0 truncate font-mono text-[10px]" title={clip.fileName}>{clip.fileName}</span>
		</span>
		<span class="obs-list-arrow shrink-0 font-mono transition-transform group-hover:translate-x-1" aria-hidden="true"
			>→</span
		>
	</button>

	<ActionMenu
		items={actionItems}
		label="More actions"
		title={`Actions for ${clip.fileName}`}
		{busy}
		bind:open={menuOpen}
		onOpenChange={onMenuOpenChange}
		triggerClass="min-h-16 w-10 rounded px-2 font-mono text-lg"
	/>
</div>
