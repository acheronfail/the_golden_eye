<script lang="ts">
	import type { SingleSegmentSnapshot, SingleSegmentSplit } from '$lib/api';

	let { snapshot, liveTotalSecs = null }: { snapshot: SingleSegmentSnapshot; liveTotalSecs?: number | null } = $props();

	const formatTime = (secs: number | null | undefined, precision = 0): string => {
		if (secs == null) return '—';
		const total = Math.max(0, secs);
		const minutes = Math.floor(total / 60);
		const seconds = total - minutes * 60;
		return `${minutes}:${seconds.toFixed(precision).padStart(precision > 0 ? 4 : 2, '0')}`;
	};

	const rowClass = (split: SingleSegmentSplit): string => {
		switch (split.status) {
			case 'complete':
				return 'obs-phase-gold-text';
			case 'active':
				return 'obs-phase-recording-text';
			default:
				return 'obs-dim';
		}
	};

	let hasActive = $derived(snapshot.splits.some((split) => split.status === 'active'));
	let rows = $derived(snapshot.splits.filter((split) => split.status !== 'pending' || hasActive));
	let visibleRows = $derived(rows.length > 0 ? rows : snapshot.splits.slice(0, 8));
</script>

<section class="w-full font-mono text-xs">
	<div class="flex items-end justify-between gap-3 px-2 pb-1">
		<div>
			<p class="obs-dim tracking-widest uppercase">splits</p>
			<p class="obs-muted text-sm">Total real time: {formatTime(liveTotalSecs ?? snapshot.totalRealTimeSecs, 1)}</p>
		</div>
		<p class="obs-dim text-right">game time from matched stats screen</p>
	</div>
	<div class="obs-dialog overflow-hidden border-t border-white/10">
		<table class="w-full border-collapse tabular-nums">
			<thead class="obs-dialog-header">
				<tr class="text-left tracking-widest uppercase">
					<th class="px-2 py-1 font-normal">#</th>
					<th class="px-2 py-1 font-normal">Level</th>
					<th class="px-2 py-1 font-normal">Diff</th>
					<th class="px-2 py-1 text-right font-normal">Real</th>
					<th class="px-2 py-1 text-right font-normal">Game</th>
				</tr>
			</thead>
			<tbody>
				{#each visibleRows as split}
					<tr class="border-t border-white/5 {rowClass(split)}">
						<td class="px-2 py-0.5">{split.index}</td>
						<td class="px-2 py-0.5">{split.level}</td>
						<td class="px-2 py-0.5">{split.difficulty}</td>
						<td class="px-2 py-0.5 text-right">{formatTime(split.realTimeSecs, 1)}</td>
						<td class="px-2 py-0.5 text-right">{formatTime(split.gameTimeSecs)}</td>
					</tr>
				{/each}
			</tbody>
		</table>
	</div>
</section>
