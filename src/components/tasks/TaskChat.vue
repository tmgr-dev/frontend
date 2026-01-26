<script setup lang="ts">
import { ref, computed, onMounted, watch, nextTick } from 'vue';
import { MessageCircle, Send, Sparkles, Reply, Edit2, Trash2, X } from 'lucide-vue-next';
import { Avatar, AvatarFallback, AvatarImage } from '@/components/ui/avatar';
import { Button } from '@/components/ui/button';
import { Textarea } from '@/components/ui/textarea';
import {
	getComments,
	createComment,
	createAskingHelpComment,
	updateComment,
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
		avatar?: string;
	};
	created_at: string;
	updated_at: string;
	parent_comment?: ChatComment;
}

interface Props {
	taskId: number;
	showCloseButton?: boolean;
}

const props = withDefaults(defineProps<Props>(), {
	showCloseButton: false,
});
const emit = defineEmits<{
	(e: 'comments-loaded', count: number): void;
	(e: 'close'): void;
}>();

const comments = ref<ChatComment[]>([]);
const newMessage = ref('');
const isLoading = ref(false);
const isSending = ref(false);
const replyingTo = ref<ChatComment | null>(null);
const editingComment = ref<ChatComment | null>(null);
const messagesContainerRef = ref<HTMLDivElement | null>(null);

const currentUser = computed(() => store.state.user);

const commentsCount = computed(() => comments.value.length);

const loadComments = async () => {
	if (!props.taskId) return;
	
	isLoading.value = true;
	try {
		const data = await getComments(props.taskId);
		comments.value = data as any;
		emit('comments-loaded', comments.value.length);
		nextTick(() => scrollToBottom());
	} catch (error) {
		console.error('Failed to load comments:', error);
	} finally {
		isLoading.value = false;
	}
};

const sendMessage = async () => {
	if (!newMessage.value.trim() || isSending.value) return;

	isSending.value = true;
	try {
		if (editingComment.value) {
			await updateComment(editingComment.value.id, {
				message: newMessage.value.trim(),
			});
			editingComment.value = null;
		} else {
			const payload: any = {
				message: newMessage.value.trim(),
			};
			
			if (replyingTo.value) {
				payload.comment_id = replyingTo.value.id;
			}

			await createComment(props.taskId, payload);
			replyingTo.value = null;
		}

		newMessage.value = '';
		await loadComments();
	} catch (error) {
		console.error('Failed to send message:', error);
	} finally {
		isSending.value = false;
	}
};

const askAI = async () => {
	if (!newMessage.value.trim() || isSending.value) return;

	isSending.value = true;
	try {
		await createAskingHelpComment(props.taskId, newMessage.value.trim());
		newMessage.value = '';
		await loadComments();
	} catch (error) {
		console.error('Failed to ask AI:', error);
	} finally {
		isSending.value = false;
	}
};

const handleReply = (comment: ChatComment) => {
	replyingTo.value = comment;
	nextTick(() => {
		const textarea = document.querySelector('textarea');
		textarea?.focus();
	});
};

const handleEdit = (comment: ChatComment) => {
	editingComment.value = comment;
	newMessage.value = comment.message;
	nextTick(() => {
		const textarea = document.querySelector('textarea');
		textarea?.focus();
	});
};

const handleDelete = async (commentId: number) => {
	if (!confirm('Delete this comment?')) return;

	try {
		await deleteComment(commentId);
		await loadComments();
	} catch (error) {
		console.error('Failed to delete comment:', error);
	}
};

const cancelReply = () => {
	replyingTo.value = null;
};

const cancelEdit = () => {
	editingComment.value = null;
	newMessage.value = '';
};

const scrollToBottom = () => {
	if (messagesContainerRef.value) {
		messagesContainerRef.value.scrollTop = messagesContainerRef.value.scrollHeight;
	}
};

const isOwnComment = (comment: ChatComment) => {
	return comment.user.id === currentUser.value?.id;
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
</script>

<template>
	<div style="display: flex; flex-direction: column; height: 100%; overflow: hidden;">
		<!-- Header -->
		<div style="flex-shrink: 0; padding: 1rem;">
			<div class="flex items-center justify-between">
				<div class="flex items-center gap-2">
					<MessageCircle class="h-5 w-5 text-gray-600 dark:text-gray-400" />
					<h3 class="font-semibold text-gray-900 dark:text-white">Comments</h3>
					<span
						v-if="commentsCount > 0"
						class="flex items-center justify-center rounded-full bg-tmgr-blue px-2 py-0.5 text-xs text-white"
					>
						{{ commentsCount }}
					</span>
				</div>
				<button 
					v-if="showCloseButton"
					@click="emit('close')"
					class="p-1.5 rounded-lg hover:bg-gray-100 dark:hover:bg-gray-800 text-gray-600 dark:text-gray-400 transition-colors"
				>
					<X class="h-5 w-5" />
				</button>
			</div>
		</div>

		<!-- Messages -->
		<div class="messages-wrapper" style="flex: 1; min-height: 0; position: relative;">
			<div ref="messagesContainerRef" style="height: 100%; overflow-y: auto; overflow-x: hidden; padding: 1rem;">
			<div v-if="isLoading" class="flex items-center justify-center py-8">
				<div class="h-6 w-6 animate-spin rounded-full border-2 border-tmgr-blue border-t-transparent"></div>
			</div>

			<div v-else-if="comments.length === 0" class="flex flex-col items-center justify-center py-12 text-center">
				<MessageCircle class="h-12 w-12 text-gray-300 dark:text-gray-700 mb-3" />
				<p class="text-sm text-gray-500 dark:text-gray-400">No comments yet</p>
				<p class="text-xs text-gray-400 dark:text-gray-500 mt-1">Start the conversation</p>
			</div>

			<div v-else class="space-y-4">
				<div
					v-for="comment in comments"
					:key="comment.id"
					class="group flex gap-3"
					:class="{ 'flex-row-reverse': isOwnComment(comment) }"
				>
					<Avatar class="h-8 w-8 flex-shrink-0">
						<AvatarImage v-if="comment.user.avatar" :src="comment.user.avatar" :alt="comment.user.name" />
						<AvatarFallback class="text-xs">
							{{ getUserInitials(comment.user.name) }}
						</AvatarFallback>
					</Avatar>

					<div class="flex flex-1 flex-col gap-1 min-w-0" :class="{ 'items-end': isOwnComment(comment) }">
						<div class="flex items-center gap-2" :class="{ 'flex-row-reverse': isOwnComment(comment) }">
							<span class="text-sm font-medium text-gray-900 dark:text-white">
								{{ comment.user.name }}
							</span>
							<span class="text-xs text-gray-500 dark:text-gray-400">
								{{ formatRelativeTime(new Date(comment.created_at)) }}
							</span>
						</div>

						<div
							v-if="comment.parent_comment"
							class="mb-1 rounded-lg border-l-2 border-gray-300 dark:border-gray-700 bg-gray-50 dark:bg-gray-800/50 px-3 py-1.5 text-xs max-w-[85%]"
							:class="{ 'border-r-2 border-l-0': isOwnComment(comment) }"
						>
							<div class="font-medium text-gray-700 dark:text-gray-300">
								{{ comment.parent_comment.user.name }}
							</div>
							<div class="text-gray-600 dark:text-gray-400 line-clamp-2">
								{{ comment.parent_comment.message }}
							</div>
						</div>

						<div
							class="relative rounded-2xl px-4 py-2 max-w-[85%]"
							:class="
								isOwnComment(comment)
									? 'bg-tmgr-blue text-white'
									: 'bg-gray-100 dark:bg-gray-800 text-gray-900 dark:text-white'
							"
							style="overflow-wrap: break-word; word-break: break-word;"
						>
							<p class="break-words text-sm whitespace-pre-wrap">{{ comment.message }}</p>

							<div
								v-if="isOwnComment(comment)"
								class="absolute -left-20 top-1/2 -translate-y-1/2 opacity-0 group-hover:opacity-100 transition-opacity flex gap-1"
							>
								<Button
									variant="ghost"
									size="icon"
									class="h-7 w-7 hover:bg-gray-100 dark:hover:bg-gray-800"
									@click="handleEdit(comment)"
								>
									<Edit2 class="h-3.5 w-3.5" />
								</Button>
								<Button
									variant="ghost"
									size="icon"
									class="h-7 w-7 hover:bg-red-100 dark:hover:bg-red-900/20 hover:text-red-600"
									@click="handleDelete(comment.id)"
								>
									<Trash2 class="h-3.5 w-3.5" />
								</Button>
							</div>

							<div
								v-else
								class="absolute -right-10 top-1/2 -translate-y-1/2 opacity-0 group-hover:opacity-100 transition-opacity"
							>
								<Button
									variant="ghost"
									size="icon"
									class="h-7 w-7 hover:bg-gray-100 dark:hover:bg-gray-800"
									@click="handleReply(comment)"
								>
									<Reply class="h-3.5 w-3.5" />
								</Button>
							</div>
						</div>
					</div>
				</div>
			</div>
			</div>
		</div>

		<!-- Input -->
		<div style="flex-shrink: 0; padding: 1rem;">
			<div v-if="replyingTo" class="mb-2 flex items-center justify-between rounded-lg bg-gray-100 dark:bg-gray-800 px-3 py-2">
				<div class="flex-1">
					<div class="text-xs font-medium text-gray-700 dark:text-gray-300">
						Replying to {{ replyingTo.user.name }}
					</div>
					<div class="text-xs text-gray-600 dark:text-gray-400 line-clamp-1">
						{{ replyingTo.message }}
					</div>
				</div>
				<Button
					variant="ghost"
					size="icon"
					class="h-6 w-6 flex-shrink-0"
					@click="cancelReply"
				>
					<span class="text-lg">&times;</span>
				</Button>
			</div>

			<div v-if="editingComment" class="mb-2 flex items-center justify-between rounded-lg bg-blue-100 dark:bg-blue-900/20 px-3 py-2">
				<div class="text-xs font-medium text-blue-700 dark:text-blue-300">
					Editing comment
				</div>
				<Button
					variant="ghost"
					size="icon"
					class="h-6 w-6 flex-shrink-0"
					@click="cancelEdit"
				>
					<span class="text-lg">&times;</span>
				</Button>
			</div>

			<div class="flex gap-2">
				<Textarea
					v-model="newMessage"
					placeholder="Type a message..."
					class="min-h-[40px] max-h-[120px] resize-none"
					@keydown.enter.exact.prevent="sendMessage"
					@keydown.enter.shift.exact="newMessage += '\n'"
				/>
				<div class="flex flex-col gap-1">
					<Button
						size="icon"
						:disabled="!newMessage.trim() || isSending"
						@click="sendMessage"
						class="flex-shrink-0"
					>
						<Send class="h-4 w-4" />
					</Button>
					<Button
						size="icon"
						variant="outline"
						:disabled="!newMessage.trim() || isSending"
						@click="askAI"
						class="flex-shrink-0"
						title="Ask AI"
					>
						<Sparkles class="h-4 w-4" />
					</Button>
				</div>
			</div>
		</div>
	</div>
</template>

<style scoped>
.messages-wrapper::before,
.messages-wrapper::after {
	content: '';
	position: absolute;
	left: 0;
	right: 0;
	height: 24px;
	pointer-events: none;
	z-index: 1;
}

.messages-wrapper::before {
	top: 0;
	background: linear-gradient(to bottom, hsl(var(--background)), transparent);
}

.messages-wrapper::after {
	bottom: 0;
	background: linear-gradient(to top, hsl(var(--background)), transparent);
}
</style>
