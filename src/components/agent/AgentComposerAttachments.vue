<template>
	<div class="mb-1.5 flex items-start gap-1.5">
		<button
			type="button"
			class="flex h-7 w-7 flex-shrink-0 items-center justify-center rounded-pill text-ink-subtle hover:bg-surface-hover hover:text-ink disabled:opacity-40 dark:hover:bg-gray-700"
			aria-label="Attach files"
			:title="`Attach files (${hint})`"
			:disabled="disabled"
			data-testid="attach-button"
			@click="pick"
		>
			<Paperclip class="h-3.5 w-3.5" />
		</button>
		<input
			ref="fileInput"
			type="file"
			multiple
			class="hidden"
			:accept="accept"
			@change="onPicked"
		/>
		<AttachmentChips
			class="min-w-0 flex-1"
			:items="queue.items.value"
			:notice="queue.notice.value"
			@remove="queue.remove"
		/>
	</div>
</template>

<script lang="ts">
	import { attachConversationFile } from '@/actions/tmgr/agent';
	import {
		presignUpload,
		putToStorageWithProgress,
	} from '@/actions/tmgr/files';
	import AttachmentChips from '@/components/attachments/AttachmentChips.vue';
	import { useAttachmentQueue } from '@/composable/useAttachmentQueue';
	import {
		ATTACHMENT_ACCEPT,
		SUPPORTED_ATTACHMENTS_HINT,
	} from '@/utils/aiAttachments';
	import { Paperclip } from 'lucide-vue-next';
	import type { PropType } from 'vue';
	import { defineComponent, ref, watch } from 'vue';

	export default defineComponent({
		name: 'AgentComposerAttachments',
		components: { AttachmentChips, Paperclip },
		props: {
			conversationId: {
				type: Number as PropType<number | null>,
				default: null,
			},
			workspaceId: { type: Number as PropType<number | null>, default: null },
			ensureConversation: {
				type: Function as PropType<() => Promise<number | null>>,
				required: true,
			},
			disabled: { type: Boolean, default: false },
		},
		setup(props) {
			const fileInput = ref<HTMLInputElement | null>(null);

			const queue = useAttachmentQueue(async (file, onProgress) => {
				const conversationId = await props.ensureConversation();
				if (!conversationId) throw new Error('No active conversation');
				const target = await presignUpload(
					file,
					props.workspaceId ?? undefined,
				);
				await putToStorageWithProgress(target, file, onProgress);
				return attachConversationFile(conversationId, file, target);
			});

			watch(
				() => props.conversationId,
				() => queue.clear(),
			);

			const pick = () => fileInput.value?.click();

			const onPicked = (event: Event) => {
				const input = event.target as HTMLInputElement;
				const files = Array.from(input.files ?? []);
				input.value = '';
				if (files.length) queue.add(files);
			};

			const setNotice = (message: string | null) => {
				queue.notice.value = message;
			};

			return {
				fileInput,
				queue,
				add: queue.add,
				clear: queue.clear,
				fileIds: queue.fileIds,
				readyFiles: queue.readyFiles,
				canSend: queue.canSend,
				uploading: queue.uploading,
				setNotice,
				pick,
				onPicked,
				accept: ATTACHMENT_ACCEPT,
				hint: SUPPORTED_ATTACHMENTS_HINT,
			};
		},
	});
</script>
