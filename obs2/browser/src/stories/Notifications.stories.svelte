<script module lang="ts">
	import { defineMeta } from '@storybook/addon-svelte-csf';
	import NotificationStory from './NotificationStory.svelte';
	import { notificationFixtures, notificationScenarios } from './fixtures';

	const { Story } = defineMeta({
		title: 'App/Notifications',
		component: NotificationStory,
		parameters: { layout: 'fullscreen' },
		args: { flags: notificationFixtures }
	});
</script>

<Story name="All tones" />
<Story name="Information" args={{ flags: notificationFixtures.slice(0, 1) }} />
<Story name="Success with action and pills" args={{ flags: notificationFixtures.slice(1, 2) }} />
<Story name="Warning" args={{ flags: notificationFixtures.slice(2, 3) }} />
<Story name="Error" args={{ flags: notificationFixtures.slice(3, 4) }} />
<Story name="Timed" args={{ flags: [{ ...notificationFixtures[0], id: 105, timeoutMs: 600_000 }] }} />
<Story name="ROM language detected" args={{ flags: notificationScenarios.languageDetected }} />
<Story name="Saving recording" args={{ flags: notificationScenarios.savingRecording }} />
<Story name="Saving failed run" args={{ flags: notificationScenarios.savingFailedRun }} />
<Story name="Failed run not saved" args={{ flags: notificationScenarios.failedRunNotSaved }} />
<Story name="Clip saved" args={{ flags: notificationScenarios.clipSaved }} />
<Story name="Failed clip saved" args={{ flags: notificationScenarios.failedClipSaved }} />
<Story name="Monitoring disabled" args={{ flags: notificationScenarios.monitoringDisabled }} />
<Story name="YouTube upload started" args={{ flags: notificationScenarios.youtubeStarted }} />
<Story name="YouTube upload failed" args={{ flags: notificationScenarios.youtubeFailed }} />
<Story name="YouTube upload completed" args={{ flags: notificationScenarios.youtubeCompleted }} />
<Story name="Config invalid" args={{ flags: notificationScenarios.configInvalid }} />
<Story name="Config reloaded" args={{ flags: notificationScenarios.configReloaded }} />
<Story name="Update available" args={{ flags: notificationScenarios.updateAvailable }} />
<Story name="Update downloading" args={{ flags: notificationScenarios.updateDownloading }} />
<Story name="Update ready" args={{ flags: notificationScenarios.updateReady }} />
<Story name="Update applying" args={{ flags: notificationScenarios.updateApplying }} />
<Story name="Update applied" args={{ flags: notificationScenarios.updateApplied }} />
<Story name="Update staging failed" args={{ flags: notificationScenarios.updateFailed }} />
<Story name="Update check failed" args={{ flags: notificationScenarios.updateCheckFailed }} />
<Story name="Update download failed" args={{ flags: notificationScenarios.updateDownloadFailed }} />
<Story name="Update apply failed" args={{ flags: notificationScenarios.updateApplyFailed }} />
<Story name="Update check result" args={{ flags: notificationScenarios.updateCheckResult }} />
