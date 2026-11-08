<template>
	<span hidden />
</template>

<script lang="ts">
	import { ToastAction, useToast } from '@/components/ui/toast';
	import { t } from '@/i18n';
	import { revealInFileManagerLabel } from '@/utils/desktop';
	import {
		downloadToast,
		type DownloadFinished,
	} from '@/utils/desktopDownloads';
	import { defineComponent, h, onBeforeUnmount, onMounted } from 'vue';

	export default defineComponent({
		name: 'DesktopDownloads',
		setup() {
			const { toast } = useToast();
			let unlisten: (() => void) | null = null;
			let disposed = false;

			const reveal = async (path: string) => {
				const { invoke } = await import('@tauri-apps/api/core');
				try {
					await invoke('reveal_download', { path });
				} catch (error) {
					toast({
						title: t('desktop.downloads.revealFailed'),
						description: String(error),
						variant: 'destructive',
					});
				}
			};

			onMounted(async () => {
				const { listen } = await import('@tauri-apps/api/event');
				const stop = await listen<DownloadFinished>(
					'download://finished',
					({ payload }) => {
						const { revealPath, ...content } = downloadToast(payload);
						toast({
							...content,
							action: revealPath
								? h(
										ToastAction,
										{
											altText: revealInFileManagerLabel(),
											onClick: () => reveal(revealPath),
										},
										() => revealInFileManagerLabel(),
									)
								: undefined,
						});
					},
				);
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
