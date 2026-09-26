<script setup lang="ts">
	import { ToastAction, useToast } from '@/components/ui/toast';
	import {
		downloadToast,
		type DownloadFinished,
	} from '@/utils/desktopDownloads';
	import { h, onBeforeUnmount, onMounted } from 'vue';

	const { toast } = useToast();
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
									altText: 'Show in Finder',
									onClick: () => reveal(revealPath),
								},
								() => 'Show in Finder',
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
</script>

<template>
	<span hidden />
</template>
