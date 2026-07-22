<script lang="ts">
	import NotificationFlags from '$lib/components/NotificationFlags.svelte';
	import { notifications, type NotificationFlag } from '$lib/stores/notifications.svelte';

	let { flags }: { flags: NotificationFlag[] } = $props();

	$effect(() => {
		notifications.flags = flags;
	});
</script>

<main class="min-h-screen px-8 py-10">
	<h1 class="obs-heading text-2xl font-semibold">Notification preview</h1>
	<p class="obs-dim mt-2 max-w-xl font-mono text-xs">
		Notifications anchor to the lower-right corner at desktop widths and lower-centre on compact docks.
	</p>
</main>
<NotificationFlags />
