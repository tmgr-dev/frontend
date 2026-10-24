<template>
	<div class="contents">
		<input
			ref="fileInput"
			type="file"
			multiple
			class="hidden"
			:accept="accept"
			@change="onPick"
		/>
		<button
			type="button"
			class="flex h-7 w-7 shrink-0 items-center justify-center rounded-pill text-ink-subtle transition hover:bg-surface-hover hover:text-ink"
			aria-label="Attach files"
			title="Attach files"
			data-testid="comment-attach"
			@click="openPicker"
		>
			<Paperclip class="h-4 w-4" />
		</button>
		<div
			v-if="items.length || notice"
			class="order-first basis-full pb-1 pt-0.5"
		>
			<AttachmentChips :items="items" :notice="notice" @remove="onRemove" />
		</div>
	</div>
</template>

<script lang="ts">
	import { detachFile } from '@/actions/tmgr/files';
	import AttachmentChips from '@/components/attachments/AttachmentChips.vue';
	import { useAttachmentQueue } from '@/composable/useAttachmentQueue';
	import { ATTACHMENT_ACCEPT } from '@/utils/aiAttachments';
	import {
		filesFromTransfer,
		uploadCommentAttachment,
	} from '@/utils/commentAttachments';
	import { Paperclip } from 'lucide-vue-next';
	import { defineComponent, ref, watch } from 'vue';

	export default defineComponent({
		name: 'CommentAttachments',
		components: { AttachmentChips, Paperclip },
		props: {
			taskId: {
				type: Number,
				required: true,
			},
		},
		setup(props) {
			const fileInput = ref<HTMLInputElement | null>(null);
			const queue = useAttachmentQueue(
				(file, onProgress) =>
					uploadCommentAttachment(props.taskId, file, onProgress),
				5,
			);

			const discard = (fileId: number | undefined) => {
				if (fileId === undefined) return;
				detachFile(fileId).catch(() => undefined);
			};

			const onRemove = (key: string) => {
				const item = queue.items.value.find((i) => i.key === key);
				queue.remove(key);
				discard(item?.file?.id);
			};

			const clear = () => queue.clear();

			watch(
				() => props.taskId,
				() => clear(),
			);

			const add = (files: Iterable<File>) => queue.add(files);

			const openPicker = () => fileInput.value?.click();

			const onPick = (event: Event) => {
				const input = event.target as HTMLInputElement;
				if (input.files?.length) add(Array.from(input.files));
				input.value = '';
			};

			const onPaste = (event: ClipboardEvent) => {
				const files = filesFromTransfer(event.clipboardData);
				if (!files.length) return;
				event.preventDefault();
				add(files);
			};

			const onDrop = (event: DragEvent) => {
				const files = filesFromTransfer(event.dataTransfer);
				if (!files.length) return;
				event.preventDefault();
				add(files);
			};

			const setNotice = (message: string | null) => {
				queue.notice.value = message;
			};

			return {
				fileInput,
				accept: ATTACHMENT_ACCEPT,
				items: queue.items,
				notice: queue.notice,
				fileIds: queue.fileIds,
				canSend: queue.canSend,
				uploading: queue.uploading,
				add,
				clear,
				setNotice,
				openPicker,
				onPick,
				onPaste,
				onDrop,
				onRemove,
			};
		},
	});
</script>
