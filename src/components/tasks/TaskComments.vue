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
		autoUpdate,
		computePosition,
		flip,
		getOverflowAncestors,
		offset,
		shift,
	} from '@floating-ui/dom';
	import {
		Bot,
		MessageCircle,
		SmilePlus,
		Sparkles,
		Trash2,
	} from 'lucide-vue-next';
	import { computed, onBeforeUnmount, onMounted, ref, watch } from 'vue';

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

		const taskId = props.taskId;
		isLoading.value = comments.value.length === 0;
		try {
			const data = (await getComments(taskId)) as unknown as ChatComment[];
			if (taskId !== props.taskId) return;
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
			if (taskId === props.taskId) isLoading.value = false;
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

		if (reactionPickerFor.value !== null) closeReactionPicker(true);

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

	let pickerEl: HTMLElement | null = null;
	let pickerAnchor: HTMLElement | null = null;
	let stopFloating: (() => void) | null = null;

	const closeReactionPicker = (restoreFocus = false) => {
		const anchor = pickerAnchor;
		reactionPickerFor.value = null;
		pickerAnchor = null;
		if (restoreFocus) anchor?.focus({ preventScroll: true });
	};

	const toggleReactionPicker = (commentId: number, event?: Event) => {
		if (reactionPickerFor.value === commentId) {
			closeReactionPicker();
			return;
		}
		pickerAnchor =
			event?.currentTarget instanceof HTMLElement ? event.currentTarget : null;
		reactionPickerFor.value = commentId;
	};

	const scrollParentOf = (el: HTMLElement) =>
		getOverflowAncestors(el).find(
			(ancestor): ancestor is HTMLElement =>
				ancestor instanceof HTMLElement &&
				/auto|scroll|overlay/.test(getComputedStyle(ancestor).overflowY),
		);

	const emojiButtons = () =>
		Array.from(
			pickerEl?.querySelectorAll<HTMLElement>('[role="menuitem"]') ?? [],
		);

	// Placed with position: fixed so the scroll container of the modal cannot clip it; it
	// stays hidden until the first computed position, then takes focus.
	const setPickerEl = (el: unknown) => {
		if (el === pickerEl) return;
		stopFloating?.();
		stopFloating = null;
		pickerEl = el instanceof HTMLElement ? el : null;
		const anchor = pickerAnchor;
		if (!pickerEl || !anchor) return;
		const floating = pickerEl;
		const boundary = scrollParentOf(anchor);
		let focused = false;
		stopFloating = autoUpdate(anchor, floating, async () => {
			const { x, y } = await computePosition(anchor, floating, {
				strategy: 'fixed',
				placement: 'top-start',
				middleware: [
					offset(4),
					flip({ boundary, rootBoundary: 'viewport', padding: 4 }),
					shift({ boundary, rootBoundary: 'viewport', padding: 4 }),
				],
			});
			floating.style.left = `${x}px`;
			floating.style.top = `${y}px`;
			floating.style.visibility = 'visible';
			if (!focused) {
				focused = true;
				emojiButtons()[0]?.focus({ preventScroll: true });
			}
		});
	};

	const onPickerKeydown = (event: KeyboardEvent) => {
		if (event.key === 'Escape') {
			event.preventDefault();
			event.stopPropagation();
			closeReactionPicker(true);
			return;
		}
		const items = emojiButtons();
		const index = items.indexOf(document.activeElement as HTMLElement);
		if (index === -1 || items.length === 0) return;
		let next: number;
		if (event.key === 'ArrowRight' || (event.key === 'Tab' && !event.shiftKey))
			next = (index + 1) % items.length;
		else if (
			event.key === 'ArrowLeft' ||
			(event.key === 'Tab' && event.shiftKey)
		)
			next = (index - 1 + items.length) % items.length;
		else if (event.key === 'Home') next = 0;
		else if (event.key === 'End') next = items.length - 1;
		else return;
		event.preventDefault();
		items[next].focus({ preventScroll: true });
	};

	// Esc must close the picker wherever focus is (a mousedown on the picker padding drops it
	// to <body>); the global modal handler skips while the menu is open.
	const onDocumentKeydown = (event: KeyboardEvent) => {
		if (event.key !== 'Escape') return;
		event.preventDefault();
		const active = document.activeElement;
		closeReactionPicker(
			active === document.body || !!pickerEl?.contains(active),
		);
	};

	// The click that follows a dismissing pointerdown on the modal backdrop would close the
	// whole modal; swallow that one click (same node as the pointerdown), nothing else.
	const swallowNextClick = (backdrop: Node) => {
		const cleanup = () => {
			window.removeEventListener('click', swallow, true);
			clearTimeout(timer);
		};
		const swallow = (event: Event) => {
			if (event.target === backdrop) {
				event.stopPropagation();
				event.preventDefault();
			}
			cleanup();
		};
		const timer = setTimeout(cleanup, 1000);
		window.addEventListener('click', swallow, true);
	};

	const onOutsidePointerDown = (event: PointerEvent) => {
		const target = event.target as Node | null;
		if (!target || pickerEl?.contains(target) || pickerAnchor?.contains(target))
			return;
		const dialog = pickerEl?.closest('[role="dialog"]');
		const onBackdrop =
			!!dialog &&
			!dialog.contains(target) &&
			!!dialog.parentElement?.contains(target);
		closeReactionPicker();
		if (onBackdrop) swallowNextClick(target);
	};

	const removeDismissListeners = () => {
		document.removeEventListener('pointerdown', onOutsidePointerDown, true);
		document.removeEventListener('keydown', onDocumentKeydown);
	};

	watch(reactionPickerFor, (id) => {
		removeDismissListeners();
		if (id === null) return;
		document.addEventListener('pointerdown', onOutsidePointerDown, true);
		document.addEventListener('keydown', onDocumentKeydown);
	});

	onBeforeUnmount(() => {
		stopFloating?.();
		removeDismissListeners();
	});

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
							class="ml-auto flex gap-1 opacity-0 transition-opacity group-focus-within:opacity-100 group-hover:opacity-100 [@media(hover:none)]:opacity-100"
						>
							<Button
								variant="ghost"
								size="icon"
								class="h-6 w-6 hover:bg-status-fix-bg hover:text-status-fix-fg"
								aria-label="Delete comment"
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
								class="inline-flex h-6 w-6 items-center justify-center rounded-full text-ink-subtle opacity-0 transition-opacity group-hover:opacity-100 hover:bg-surface-hover hover:text-ink focus-visible:opacity-100"
								:class="{ 'opacity-100': reactionPickerFor === comment.id }"
								aria-label="Add reaction"
								aria-haspopup="menu"
								:aria-expanded="reactionPickerFor === comment.id"
								@click="toggleReactionPicker(comment.id, $event)"
							>
								<SmilePlus class="h-3.5 w-3.5" />
							</button>

							<div
								v-if="reactionPickerFor === comment.id"
								:ref="setPickerEl"
								role="menu"
								data-state="open"
								aria-label="Add reaction"
								data-testid="reaction-picker"
								class="invisible fixed top-0 left-0 z-50 flex gap-0.5 rounded-lg border border-line bg-surface p-1 shadow-md"
								@keydown="onPickerKeydown"
							>
								<button
									v-for="emoji in DEFAULT_REACTION_EMOJIS"
									:key="emoji"
									type="button"
									role="menuitem"
									:aria-label="emoji"
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
