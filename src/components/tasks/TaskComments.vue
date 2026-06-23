<script setup lang="ts">
	import { ref, computed, onMounted, watch } from 'vue';
	import {
		MessageCircle,
		Edit2,
		Trash2,
		Bot,
		Sparkles,
		SmilePlus,
	} from 'lucide-vue-next';
	import { Avatar, AvatarFallback, AvatarImage } from '@/components/ui/avatar';
	import { Button } from '@/components/ui/button';
	import {
		getComments,
		deleteComment,
		toggleCommentReaction,
	} from '@/actions/tmgr/comments';
	import {
		toggleReaction,
		normalizeReactions,
		mergeServerReactionForEmoji,
		DEFAULT_REACTION_EMOJIS,
		type ReactionSummary,
	} from '@/utils/commentReactions';
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
		reactions?: ReactionSummary[];
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
	const reactionPickerFor = ref<number | null>(null);
	const reactionInFlight = new Set<string>();

	const currentUser = computed(() => store.state.user);
	const commentsCount = computed(() => comments.value.length);

	const sortedComments = computed(() => {
		return [...comments.value].sort((a, b) => {
			return (
				new Date(b.created_at).getTime() - new Date(a.created_at).getTime()
			);
		});
	});

	const loadComments = async () => {
		if (!props.taskId) return;

		isLoading.value = true;
		try {
			const data = (await getComments(
				props.taskId,
			)) as unknown as ChatComment[];
			comments.value = data.map((comment) => ({
				...comment,
				reactions: normalizeReactions(comment.reactions, currentUser.value?.id),
			}));
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

	const handleToggleReaction = async (comment: ChatComment, emoji: string) => {
		const user = currentUser.value;
		if (!user?.id) return;

		// The backend endpoint is a STATEFUL toggle: each request flips whatever
		// the server's current state is. Driving it with a client-side queue is
		// unsafe (a failed earlier toggle leaves queued ones flipping a state that
		// never changed). So allow only ONE in-flight toggle per (comment,emoji):
		// each request acts on settled, known state. Different emojis are
		// independent and merge per-emoji, so they never block or clobber.
		const key = `${comment.id}:${emoji}`;
		if (reactionInFlight.has(key)) return;
		reactionInFlight.add(key);

		reactionPickerFor.value = null;

		const previous = comment.reactions ?? [];
		comment.reactions = toggleReaction(previous, emoji, {
			id: user.id,
			name: user.name,
		});

		try {
			const server = await toggleCommentReaction(comment.id, emoji, user.id);
			comment.reactions = mergeServerReactionForEmoji(
				comment.reactions ?? [],
				server,
				emoji,
			);
		} catch (error) {
			console.error('Failed to toggle reaction:', error);
			comment.reactions = mergeServerReactionForEmoji(
				comment.reactions ?? [],
				previous,
				emoji,
			);
		} finally {
			reactionInFlight.delete(key);
		}
	};

	const toggleReactionPicker = (commentId: number) => {
		reactionPickerFor.value =
			reactionPickerFor.value === commentId ? null : commentId;
	};

	const getUserInitials = (name: string) => {
		return name
			.split(' ')
			.map((n) => n[0])
			.join('')
			.toUpperCase()
			.slice(0, 2);
	};

	onMounted(() => {
		loadComments();
	});

	watch(
		() => props.taskId,
		() => {
			loadComments();
		},
	);

	defineExpose({
		loadComments,
	});
</script>

<template>
	<div class="border-t border-gray-200 pt-4 dark:border-gray-700">
		<!-- Header -->
		<div class="mb-3 flex items-center gap-2">
			<MessageCircle class="h-4 w-4 text-gray-600 dark:text-gray-400" />
			<span class="text-sm font-medium text-gray-900 dark:text-white"
				>Activity</span
			>
			<span
				v-if="commentsCount > 0"
				class="text-xs text-gray-500 dark:text-gray-400"
			>
				{{ commentsCount }}
			</span>
		</div>

		<!-- Comments List -->
		<div v-if="isLoading" class="flex items-center justify-center py-4">
			<div
				class="h-5 w-5 animate-spin rounded-full border-2 border-tmgr-blue border-t-transparent"
			></div>
		</div>

		<div
			v-else-if="comments.length === 0"
			class="py-2 text-xs text-gray-500 dark:text-gray-400"
		>
			No comments yet
		</div>

		<div v-else class="space-y-3">
			<div
				v-for="comment in sortedComments"
				:key="comment.id"
				:class="[
					'group -ml-3 flex gap-2 rounded-lg p-3',
					comment.cursor_agent_id
						? 'border-l-4 border-violet-500 bg-violet-50 dark:bg-violet-900/20'
						: comment.user?.email === 'ai@tmgr.dev'
						? 'border-l-4 border-blue-500 bg-blue-50 dark:bg-blue-900/20'
						: '',
				]"
			>
				<template
					v-if="
						comment.cursor_agent_id &&
						comment.cursor_message_type === 'assistant_message'
					"
				>
					<div
						class="mt-0.5 flex h-7 w-7 flex-shrink-0 items-center justify-center rounded-full bg-violet-100 dark:bg-violet-900/30"
					>
						<Bot class="h-4 w-4 text-violet-600 dark:text-violet-400" />
					</div>
				</template>
				<template v-else-if="comment.user?.email === 'ai@tmgr.dev'">
					<div
						class="mt-0.5 flex h-7 w-7 flex-shrink-0 items-center justify-center rounded-full bg-blue-100 dark:bg-blue-900/30"
					>
						<Sparkles class="h-4 w-4 text-blue-600 dark:text-blue-400" />
					</div>
				</template>
				<Avatar v-else class="mt-0.5 h-7 w-7 flex-shrink-0">
					<AvatarImage
						v-if="comment.user.avatar"
						:src="comment.user.avatar"
						:alt="comment.user.name"
					/>
					<AvatarFallback class="text-xs">
						{{ getUserInitials(comment.user.name) }}
					</AvatarFallback>
				</Avatar>

				<div class="min-w-0 flex-1">
					<div class="mb-1 flex items-center gap-2">
						<span
							v-if="
								comment.cursor_agent_id &&
								comment.cursor_message_type === 'assistant_message'
							"
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
							v-if="
								comment.user.id === currentUser?.id && !comment.cursor_agent_id
							"
							class="ml-auto flex gap-1 opacity-0 transition-opacity group-hover:opacity-100"
						>
							<Button
								variant="ghost"
								size="icon"
								class="h-6 w-6 hover:bg-red-100 hover:text-red-600 dark:hover:bg-red-900/20"
								@click="handleDelete(comment.id)"
							>
								<Trash2 class="h-3 w-3" />
							</Button>
						</div>
					</div>

					<div
						class="whitespace-pre-wrap break-words text-sm text-gray-700 dark:text-gray-300"
					>
						{{ comment.message }}
					</div>

					<div class="mt-1.5 flex flex-wrap items-center gap-1">
						<button
							v-for="reaction in comment.reactions"
							:key="reaction.emoji"
							type="button"
							:title="(reaction.users || []).map((u) => u.name).join(', ')"
							:class="[
								'inline-flex items-center gap-1 rounded-full border px-1.5 py-0.5 text-xs transition-colors',
								reaction.reacted
									? 'border-tmgr-blue bg-tmgr-blue/10 text-tmgr-blue dark:border-tmgr-light-blue dark:text-tmgr-light-blue'
									: 'border-gray-200 bg-gray-50 text-gray-600 hover:bg-gray-100 dark:border-gray-700 dark:bg-gray-800 dark:text-gray-300 dark:hover:bg-gray-700',
							]"
							@click="handleToggleReaction(comment, reaction.emoji)"
						>
							<span>{{ reaction.emoji }}</span>
							<span class="tabular-nums">{{ reaction.count }}</span>
						</button>

						<div class="relative">
							<button
								type="button"
								class="inline-flex h-6 w-6 items-center justify-center rounded-full text-gray-400 opacity-0 transition-opacity hover:bg-gray-100 hover:text-gray-600 group-hover:opacity-100 dark:hover:bg-gray-700 dark:hover:text-gray-200"
								:class="{ 'opacity-100': reactionPickerFor === comment.id }"
								aria-label="Add reaction"
								@click="toggleReactionPicker(comment.id)"
							>
								<SmilePlus class="h-3.5 w-3.5" />
							</button>

							<div
								v-if="reactionPickerFor === comment.id"
								class="absolute left-0 z-10 mt-1 flex gap-0.5 rounded-lg border border-gray-200 bg-white p-1 shadow-md dark:border-gray-700 dark:bg-gray-800"
							>
								<button
									v-for="emoji in DEFAULT_REACTION_EMOJIS"
									:key="emoji"
									type="button"
									class="rounded-md px-1.5 py-1 text-base leading-none transition-colors hover:bg-gray-100 dark:hover:bg-gray-700"
									@click="handleToggleReaction(comment, emoji)"
								>
									{{ emoji }}
								</button>
							</div>
						</div>
					</div>
				</div>
			</div>
		</div>
	</div>
</template>
