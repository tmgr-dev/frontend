<script setup lang="ts">
	import {
		deleteComment,
		getComments,
		toggleCommentReaction,
	} from '@/actions/tmgr/comments';
	import AttachedFileList from '@/components/attachments/AttachedFileList.vue';
	import AuthorBadge from '@/components/general/AuthorBadge.vue';
	import MarkdownText from '@/components/general/MarkdownText.vue';
	import MemberAvatarLink from '@/components/general/MemberAvatarLink.vue';
	import UserAvatar from '@/components/general/UserAvatar.vue';
	import { Avatar } from '@/components/ui/avatar';
	import { Button } from '@/components/ui/button';
	import store from '@/store';
	import type { AttachedFile } from '@/utils/aiAttachments';
	import { sortCommentsOldestFirst } from '@/utils/commentOrder';
	import {
		applyReactionsUpdate,
		type CommentReactionsUpdatedEvent,
		DEFAULT_REACTION_EMOJIS,
		mergeServerReactionForEmoji,
		normalizeReactions,
		type ReactionSummary,
		toggleReaction,
	} from '@/utils/commentReactions';
	import {
		type AuthorFilter,
		isMachineAuthored,
		matchesAuthorFilter,
	} from '@/utils/personas';
	import { formatRelativeTime } from '@/utils/timeUtils';
	import {
		Bot,
		MessageCircle,
		SmilePlus,
		Sparkles,
		Trash2,
	} from 'lucide-vue-next';
	import { computed, onMounted, ref, watch } from 'vue';

	interface ChatComment {
		id: number;
		message: string;
		user: {
			id: number;
			name: string;
			email?: string;
			avatar?: string;
			has_avatar?: boolean;
		};
		created_at: string;
		updated_at: string;
		parent_comment?: ChatComment;
		cursor_agent_id?: number | null;
		cursor_message_id?: string | null;
		cursor_message_type?: 'user_message' | 'assistant_message' | null;
		reactions?: ReactionSummary[];
		files?: AttachedFile[];
		author?: {
			kind: string;
			id: string;
			name: string;
			owner?: { id: string; name: string };
			avatar?: string;
		} | null;
	}

	interface Props {
		taskId: number;
		hideHeader?: boolean;
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
	const authorFilter = ref<AuthorFilter>('all');
	const authorFilterOptions: { value: AuthorFilter; label: string }[] = [
		{ value: 'all', label: 'All' },
		{ value: 'people', label: 'People' },
		{ value: 'personas', label: 'Personas' },
		{ value: 'plugins', label: 'Plugins' },
	];

	const currentUser = computed(() => store.state.user);
	const commentsCount = computed(() => comments.value.length);

	// Both the page rail and the modal read chronologically, so the newest
	// comment is the one next to the composer (TM-144).
	const sortedComments = computed(() =>
		sortCommentsOldestFirst(comments.value),
	);

	const hasNonUserAuthors = computed(() =>
		comments.value.some((c) => c.author && c.author.kind !== 'user'),
	);

	const filteredComments = computed(() =>
		sortedComments.value.filter((c) =>
			matchesAuthorFilter(c, authorFilter.value),
		),
	);

	const loadComments = async () => {
		if (!props.taskId) return;

		isLoading.value = comments.value.length === 0;
		try {
			const data = (await getComments(
				props.taskId,
			)) as unknown as ChatComment[];
			const known = new Map(comments.value.map((c) => [c.id, c]));
			comments.value = data.map((comment) => {
				const next = {
					...comment,
					reactions: normalizeReactions(
						comment.reactions,
						currentUser.value?.id,
					),
				};
				const previous = known.get(next.id);
				return previous && JSON.stringify(previous) === JSON.stringify(next)
					? previous
					: next;
			});
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

	const AVATAR_COLORS = [
		'#14b8a6',
		'#6366f1',
		'#22c55e',
		'#8b5cf6',
		'#ec4899',
		'#f59e0b',
		'#ef4444',
		'#0ea5e9',
	];
	const getAvatarColor = (user: { id?: number; name?: string }) => {
		const key = String(user?.id ?? user?.name ?? '');
		let hash = 0;
		for (let i = 0; i < key.length; i++) {
			hash = (hash * 31 + key.charCodeAt(i)) >>> 0;
		}
		return AVATAR_COLORS[hash % AVATAR_COLORS.length];
	};

	onMounted(() => {
		loadComments();
	});

	watch(
		() => props.taskId,
		() => {
			comments.value = [];
			loadComments();
		},
	);

	/** Realtime: replace one comment's reactions in place (no reload, keeps scroll and drafts). */
	const applyReactions = (event: CommentReactionsUpdatedEvent) => {
		comments.value = applyReactionsUpdate(
			comments.value,
			event,
			currentUser.value?.id,
		);
	};

	defineExpose({
		loadComments,
		applyReactions,
	});
</script>

<template>
	<div :class="hideHeader ? '' : 'border-t border-line pt-4'">
		<!-- Header -->
		<div v-if="!hideHeader" class="mb-3 flex items-center gap-2">
			<MessageCircle class="h-4 w-4 text-ink-subtle" />
			<span class="text-sm font-medium text-ink">Activity</span>
			<span v-if="commentsCount > 0" class="text-xs text-ink-subtle">
				{{ commentsCount }}
			</span>
		</div>

		<div v-if="hasNonUserAuthors" class="mb-3 flex gap-1">
			<button
				v-for="option in authorFilterOptions"
				:key="option.value"
				type="button"
				:class="[
					'rounded-full px-2 py-0.5 text-2xs font-medium transition-colors',
					authorFilter === option.value
						? 'bg-brand text-on-accent'
						: 'bg-surface-sunken text-ink-muted hover:bg-surface-active',
				]"
				@click="authorFilter = option.value"
			>
				{{ option.label }}
			</button>
		</div>

		<!-- Comments List -->
		<div v-if="isLoading" class="flex items-center justify-center py-4">
			<div
				class="h-5 w-5 animate-spin rounded-full border-2 border-brand border-t-transparent"
			></div>
		</div>

		<div v-else-if="comments.length === 0" class="py-2 text-xs text-ink-subtle">
			No comments yet
		</div>

		<div
			v-else-if="filteredComments.length === 0"
			class="py-2 text-xs text-ink-subtle"
		>
			No comments match this filter
		</div>

		<div v-else class="space-y-3">
			<div
				v-for="comment in filteredComments"
				:key="comment.id"
				:data-comment-id="comment.id"
				:class="[
					'group flex gap-3',
					comment.cursor_agent_id
						? '-ml-3 rounded-lg border-l-4 border-status-testing bg-surface-sunken p-3'
						: comment.user?.email === 'ai@tmgr.dev'
							? '-ml-3 rounded-lg border-l-4 border-brand bg-surface-sunken p-3'
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
						class="mt-0.5 flex h-7 w-7 shrink-0 items-center justify-center rounded-full bg-status-testing-bg"
					>
						<Bot class="h-4 w-4 text-status-testing-fg" />
					</div>
				</template>
				<template v-else-if="comment.user?.email === 'ai@tmgr.dev'">
					<div
						class="mt-0.5 flex h-7 w-7 shrink-0 items-center justify-center rounded-full bg-brand-bg"
					>
						<Sparkles class="h-4 w-4 text-brand" />
					</div>
				</template>
				<span
					v-else-if="comment.author && comment.author.kind !== 'user'"
					class="mt-0.5 shrink-0"
				>
					<AuthorBadge
						:author="comment.author"
						:size="28"
						:show-label="false"
					/>
				</span>
				<MemberAvatarLink
					v-else
					:user-id="comment.user.id"
					:label="comment.user.name"
					class="mt-0.5 h-7 w-7 shrink-0"
				>
					<Avatar class="h-7 w-7">
						<UserAvatar
							:user-id="comment.user.id"
							:name="comment.user.name"
							:has-avatar="comment.user.has_avatar ?? false"
							:size="28"
							class="text-2xs font-bold text-white"
							:style="{ backgroundColor: getAvatarColor(comment.user) }"
						/>
					</Avatar>
				</MemberAvatarLink>

				<div class="min-w-0 flex-1">
					<div class="mb-1 flex items-center gap-2">
						<span
							v-if="
								comment.cursor_agent_id &&
								comment.cursor_message_type === 'assistant_message'
							"
							class="text-sm font-semibold text-status-testing-fg"
						>
							Cursor Agent
						</span>
						<span
							v-else-if="comment.user?.email === 'ai@tmgr.dev'"
							class="text-sm font-semibold text-brand"
						>
							AI Assistant
						</span>
						<AuthorBadge
							v-else-if="comment.author && comment.author.kind !== 'user'"
							:author="comment.author"
							hide-avatar
						/>
						<span v-else class="text-sm font-semibold text-ink">
							{{ comment.user.name }}
						</span>
						<span class="text-2xs text-ink-faint">
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
								class="h-6 w-6 hover:bg-status-fix-bg hover:text-status-fix-fg"
								@click="handleDelete(comment.id)"
							>
								<Trash2 class="h-3 w-3" />
							</Button>
						</div>
					</div>

					<MarkdownText
						:content="comment.message"
						:untrusted="isMachineAuthored(comment)"
					/>

					<AttachedFileList
						v-if="comment.files?.length"
						:files="comment.files"
						class="mt-1.5"
					/>

					<div class="mt-1.5 flex flex-wrap items-center gap-1">
						<button
							v-for="reaction in comment.reactions"
							:key="reaction.emoji"
							type="button"
							:title="(reaction.users || []).map((u) => u.name).join(', ')"
							:class="[
								'inline-flex items-center gap-1 rounded-full border px-1.5 py-0.5 text-xs transition-colors',
								reaction.reacted
									? 'border-brand bg-brand-bg text-brand'
									: 'border-line bg-surface-sunken text-ink-muted hover:bg-surface-hover',
							]"
							@click="handleToggleReaction(comment, reaction.emoji)"
						>
							<span>{{ reaction.emoji }}</span>
							<span class="tabular-nums">{{ reaction.count }}</span>
						</button>

						<div class="relative">
							<button
								type="button"
								class="inline-flex h-6 w-6 items-center justify-center rounded-full text-ink-subtle opacity-0 transition-opacity group-hover:opacity-100 hover:bg-surface-hover hover:text-ink"
								:class="{ 'opacity-100': reactionPickerFor === comment.id }"
								aria-label="Add reaction"
								@click="toggleReactionPicker(comment.id)"
							>
								<SmilePlus class="h-3.5 w-3.5" />
							</button>

							<div
								v-if="reactionPickerFor === comment.id"
								class="absolute left-0 z-10 mt-1 flex gap-0.5 rounded-lg border border-line bg-surface p-1 shadow-md"
							>
								<button
									v-for="emoji in DEFAULT_REACTION_EMOJIS"
									:key="emoji"
									type="button"
									class="rounded-md px-1.5 py-1 text-base leading-none transition-colors hover:bg-surface-hover"
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
