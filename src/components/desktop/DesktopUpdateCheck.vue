<template>
	<span hidden />
</template>

<script lang="ts">
	import { ToastAction, useToast } from '@/components/ui/toast';
	import {
		checkForUpdateManually,
		installUpdate,
	} from '@/utils/desktopUpdater';
	import { updateCheckToast } from '@/utils/desktopUpdateCheck';
	import { defineComponent, h, onBeforeUnmount, onMounted } from 'vue';

	export default defineComponent({
		name: 'DesktopUpdateCheck',
		setup() {
			const { toast } = useToast();
			let unlisten: (() => void) | null = null;
			let disposed = false;

			const runManualCheck = async () => {
				const outcome = await checkForUpdateManually();
				let currentVersion = '';
				try {
					const { getVersion } = await import('@tauri-apps/api/app');
					currentVersion = await getVersion();
				} catch {
					currentVersion = '';
				}
				const content = updateCheckToast(outcome, currentVersion);
				toast({
					title: content.title,
					description: content.description,
					variant: content.variant,
					action: content.installable
						? h(
								ToastAction,
								{
									altText: 'Install and restart',
									onClick: () => installUpdate(),
								},
								() => 'Install and restart',
						  )
						: undefined,
				});
			};

			onMounted(async () => {
				const { listen } = await import('@tauri-apps/api/event');
				const stop = await listen('menu://check-for-updates', () => {
					runManualCheck();
				});
				if (disposed) stop();
				else unlisten = stop;
			});

			onBeforeUnmount(() => {
				disposed = true;
				unlisten?.();
			});
		},
	});
</script>
