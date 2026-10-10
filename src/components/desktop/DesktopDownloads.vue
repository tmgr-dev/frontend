<template>
	<span hidden />
</template>

<script lang="ts">
	import { ToastAction, useToast } from '@/components/ui/toast';
	import {
		downloadToast,
		type DownloadFinished,
	} from '@/utils/desktopDownloads';
	import { revealInFileManagerLabel } from '@/utils/desktop';
	import { defineComponent, h, onBeforeUnmount, onMounted } from 'vue';

	export default defineComponent({
		name: 'DesktopDownloads',
		setup() {
			const { toast } = useToast();
			const revealLabel = revealInFileManagerLabel();
			let unlisten: (() => void) | null = null;
			let disposed = false;

			const reveal = async (path: string) => {
				const { invoke } = await import('@tauri-apps/api/core');
				try {
					await invoke('reveal_download', { path });
				} catch (error) {
					toast({
						title: 'Could not show the file',
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
											altText: revealLabel,
											onClick: () => reveal(revealPath),
										},
										() => revealLabel,
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
