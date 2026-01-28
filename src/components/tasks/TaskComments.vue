<script setup lang="ts">
import { ref, computed, onMounted, watch } from 'vue';
import { MessageCircle, Edit2, Trash2, Bot, Sparkles } from 'lucide-vue-next';
import { Avatar, AvatarFallback, AvatarImage } from '@/components/ui/avatar';
import { Button } from '@/components/ui/button';
import {
	getComments,
	deleteComment,
} from '@/actions/tmgr/comments';
import { formatRelativeTime } from '@/utils/timeUtils';
import store from '@/store';

interface ChatComment {
	id: number;
	message: string;
	user: {
		id: number;
		name: string;
		email?: string;
		avatar?: string;
	};
	created_at: string;
	updated_at: string;
	parent_comment?: ChatComment;
	cursor_agent_id?: number | null;
	cursor_message_id?: string | null;
	cursor_message_type?: 'user_message' | 'assistant_message' | null;
}

interface Props {
	taskId: number;
}

const props = defineProps<Props>();
const emit = defineEmits<{
	(e: 'reload'): void;
	(e: 'update:count', count: number): void;
}>();

const comments = ref<ChatComment[]>([]);
const isLoading = ref(false);

const currentUser = computed(() => store.state.user);
const commentsCount = computed(() => comments.value.length);

const sortedComments = computed(() => {
	return [...comments.value].sort((a, b) => {
		return new Date(b.created_at).getTime() - new Date(a.created_at).getTime();
	});
});

const loadComments = async () => {
	if (!props.taskId) return;
	
	isLoading.value = true;
	try {
		const data = await getComments(props.taskId);
		comments.value = data as any;
		emit('update:count', comments.value.length);
	} catch (error) {
		console.error('Failed to load comments:', error);
	} finally {
		isLoading.value = false;
	}
};

const handleDelete = async (commentId: number) => {
	if (!confirm('Delete this comment?')) return;

	try {
		await deleteComment(commentId);
		await loadComments();
		emit('reload');
	} catch (error) {
		console.error('Failed to delete comment:', error);
	}
};

const getUserInitials = (name: string) => {
	return name
		.split(' ')
		.map(n => n[0])
		.join('')
		.toUpperCase()
		.slice(0, 2);
};

onMounted(() => {
	loadComments();
});

watch(() => props.taskId, () => {
	loadComments();
});

defineExpose({
	loadComments
});
</script>

<template>
	<div class="border-t border-gray-200 dark:border-gray-700 pt-4">
		<!-- Header -->
		<div class="flex items-center gap-2 mb-3">
			<MessageCircle class="h-4 w-4 text-gray-600 dark:text-gray-400" />
			<span class="text-sm font-medium text-gray-900 dark:text-white">Activity</span>
			<span v-if="commentsCount > 0" class="text-xs text-gray-500 dark:text-gray-400">
				{{ commentsCount }}
			</span>
		</div>

		<!-- Comments List -->
		<div v-if="isLoading" class="flex items-center justify-center py-4">
			<div class="h-5 w-5 animate-spin rounded-full border-2 border-tmgr-blue border-t-transparent"></div>
		</div>

		<div v-else-if="comments.length === 0" class="py-2 text-xs text-gray-500 dark:text-gray-400">
			No comments yet
		</div>

		<div v-else class="space-y-3">
			<div
				v-for="comment in sortedComments"
				:key="comment.id"
				:class="[
					'group flex gap-2 rounded-lg p-3 -ml-3',
					comment.cursor_agent_id
						? 'bg-violet-50 border-l-4 border-violet-500 dark:bg-violet-900/20'
						: comment.user?.email === 'ai@tmgr.dev'
						? 'bg-blue-50 border-l-4 border-blue-500 dark:bg-blue-900/20'
						: ''
				]"
			>
				<template v-if="comment.cursor_agent_id && comment.cursor_message_type === 'assistant_message'">
					<div class="h-7 w-7 flex items-center justify-center rounded-full bg-violet-100 dark:bg-violet-900/30 flex-shrink-0 mt-0.5">
						<Bot class="h-4 w-4 text-violet-600 dark:text-violet-400" />
					</div>
				</template>
				<template v-else-if="comment.user?.email === 'ai@tmgr.dev'">
					<div class="h-7 w-7 flex items-center justify-center rounded-full bg-blue-100 dark:bg-blue-900/30 flex-shrink-0 mt-0.5">
						<Sparkles class="h-4 w-4 text-blue-600 dark:text-blue-400" />
					</div>
				</template>
				<Avatar v-else class="h-7 w-7 flex-shrink-0 mt-0.5">
					<AvatarImage v-if="comment.user.avatar" :src="comment.user.avatar" :alt="comment.user.name" />
					<AvatarFallback class="text-xs">
						{{ getUserInitials(comment.user.name) }}
					</AvatarFallback>
				</Avatar>

				<div class="flex-1 min-w-0">
					<div class="flex items-center gap-2 mb-1">
						<span 
							v-if="comment.cursor_agent_id && comment.cursor_message_type === 'assistant_message'"
							class="text-sm font-semibold text-violet-600 dark:text-violet-400"
						>
							Cursor Agent
						</span>
						<span 
							v-else-if="comment.user?.email === 'ai@tmgr.dev'"
							class="text-sm font-semibold text-blue-600 dark:text-blue-400"
						>
							AI Assistant
						</span>
						<span 
							v-else
							class="text-sm font-medium text-gray-900 dark:text-white"
						>
							{{ comment.user.name }}
						</span>
						<span class="text-xs text-gray-500 dark:text-gray-400">
							{{ formatRelativeTime(new Date(comment.created_at)) }}
						</span>
						<div 
							v-if="comment.user.id === currentUser?.id && !comment.cursor_agent_id"
							class="ml-auto flex gap-1 opacity-0 group-hover:opacity-100 transition-opacity"
						>
							<Button
								variant="ghost"
								size="icon"
								class="h-6 w-6 hover:bg-red-100 dark:hover:bg-red-900/20 hover:text-red-600"
								@click="handleDelete(comment.id)"
							>
								<Trash2 class="h-3 w-3" />
							</Button>
						</div>
					</div>

					<div class="text-sm text-gray-700 dark:text-gray-300 whitespace-pre-wrap break-words">
						{{ comment.message }}
					</div>
				</div>
			</div>
		</div>
	</div>
</template>
