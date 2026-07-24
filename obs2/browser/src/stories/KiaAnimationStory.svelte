<script lang="ts">
	import KiaDeathOverlay from '$lib/components/KiaDeathOverlay.svelte';

	let { durationMs = 2000 }: { durationMs?: number } = $props();
	let trigger = $state(1);

	const replay = () => {
		trigger += 1;
	};
</script>

<main class="relative flex min-h-screen items-center justify-center overflow-hidden bg-[#080a10] px-6 text-center">
	<div
		class="absolute inset-0 bg-[radial-gradient(circle_at_center,#2d313d_0%,#151821_48%,#050608_100%)] opacity-50"
	></div>
	<div class="relative z-10 grid max-w-lg justify-items-center gap-4">
		<p class="font-mono text-xs tracking-[0.28em] text-(--obs-danger) uppercase">Killed in action</p>
		<h1 class="text-4xl font-semibold obs-heading">Mission failed</h1>
		<p class="text-sm obs-muted">The overlay starts automatically when the story opens.</p>
		<button type="button" class="mt-3 obs-button obs-button-danger px-4 py-2 font-mono text-sm" onclick={replay}>
			Replay KIA animation
		</button>
	</div>
</main>

<KiaDeathOverlay {trigger} {durationMs} />
