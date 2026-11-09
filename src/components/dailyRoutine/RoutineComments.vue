<template>
	<div>
		<TaskComments
			ref="commentsRef"
			:task-id="taskId"
			hide-header
			@update:count="$emit('count', $event)"
		/>
		<form class="mt-3 flex items-end gap-2" @submit.prevent="send">
			<textarea
				v-model="message"
				rows="2"
				class="min-h-[2.5rem] flex-1 resize-y rounded-md border border-line bg-surface px-3 py-2 text-sm text-ink outline-none focus:border-brand"
				:placeholder="$t('dailyRoutine.comments.placeholder')"
				:aria-label="$t('dailyRoutine.comments.placeholder')"
				data-testid="routine-comment-input"
				@keydown.enter.meta.prevent="send"
				@keydown.enter.ctrl.prevent="send"
			/>
			<button
				type="submit"
				class="rounded-md bg-brand px-3 py-2 text-xs font-semibold text-on-accent disabled:opacity-50"
				:disabled="!message.trim() || sending"
				data-testid="routine-comment-send"
			>
				{{ $t('dailyRoutine.comments.send') }}
			</button>
		</form>
		<p v-if="error" class="mt-1 text-xs text-status-fix-fg" role="alert">
			{{ error }}
		</p>
	</div>
</template>

<script setup lang="ts">
	import { createComment } from '@/actions/tmgr/comments';
	import TaskComments from '@/components/tasks/TaskComments.vue';
	import { t } from '@/i18n';
	import { ref } from 'vue';

	const props = defineProps<{ taskId: number }>();
	defineEmits<{ (e: 'count', count: number): void }>();

	const commentsRef = ref<InstanceType<typeof TaskComments> | null>(null);
	const message = ref('');
	const sending = ref(false);
	const error = ref('');

	async function send() {
		const text = message.value.trim();
		if (!text || sending.value) return;
		sending.value = true;
		error.value = '';
		try {
			await createComment(props.taskId, { message: text });
			message.value = '';
			await commentsRef.value?.loadComments();
		} catch {
			error.value = t('dailyRoutine.comments.sendError');
		} finally {
			sending.value = false;
		}
	}
</script>
