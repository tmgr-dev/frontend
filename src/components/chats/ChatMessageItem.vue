<template>
	<div
		class="group relative flex gap-3 rounded-md px-2 py-1.5 hover:bg-surface-hover"
		:data-testid="`chat-message-${message.id}`"
	>
		<span
			class="mt-0.5 flex h-8 w-8 shrink-0 items-center justify-center rounded-full bg-surface-hover text-xs font-semibold text-ink-subtle"
			>{{ initial }}</span
		>
		<div class="min-w-0 flex-1">
			<div class="flex items-baseline gap-2">
				<span class="text-sm font-semibold text-ink">{{
					message.author.name
				}}</span>
				<span
					v-if="isPersona"
					class="rounded bg-brand-bg px-1.5 py-0.5 text-[10px] font-semibold text-brand dark:bg-surface-hover dark:text-ink"
					data-testid="chat-persona-badge"
					>{{ $t('chats.persona.badge') }}</span
				>
				<span class="text-xs text-ink-subtle">{{ time }}</span>
				<span
					v-if="message.edited_at && !message.deleted"
					class="text-xs text-ink-subtle"
					>{{ $t('chats.edited') }}</span
				>
			</div>
			<p v-if="message.deleted" class="text-sm text-ink-subtle italic">
				{{ $t('chats.messageDeleted') }}
			</p>
			<template v-else>
				<div v-if="editing" class="mt-1">
					<textarea
						v-model="draft"
						rows="2"
						class="w-full resize-none rounded-md border border-line bg-surface p-2 text-sm text-ink outline-hidden"
						data-testid="chat-edit-input"
						@keydown="onEditKey"
					></textarea>
					<p class="text-xs text-ink-subtle">{{ $t('chats.editHint') }}</p>
				</div>
				<div
					v-else-if="message.status === 'pending'"
					class="flex items-center gap-2 text-sm text-ink-subtle"
					data-testid="chat-persona-pending"
				>
					<span class="italic">{{ $t('chats.persona.thinking') }}</span>
					<Button
						v-if="mayStop"
						variant="ghost"
						size="sm"
						data-testid="chat-persona-stop"
						@click="stop"
						>{{ $t('chats.persona.stop') }}</Button
					>
				</div>
				<p
					v-else-if="message.status === 'failed'"
					class="text-sm text-destructive"
					data-testid="chat-persona-failed"
				>
					{{ $t('chats.persona.failed') }}
				</p>
				<p
					v-else-if="message.status === 'canceled'"
					class="text-sm text-ink-subtle italic"
					data-testid="chat-persona-canceled"
				>
					{{ $t('chats.persona.stopped') }}
				</p>
				<MarkdownText
					v-else-if="message.body || !message.forward"
					:content="message.body ?? ''"
				/>
				<div
					v-if="forwardView && message.forward && !editing"
					class="mt-1 rounded-md border border-l-4 border-line border-l-brand bg-surface p-2 text-sm dark:bg-surface-hover"
					data-testid="chat-forward-card"
				>
					<div
						class="flex flex-wrap items-baseline gap-x-2 text-xs text-ink-subtle"
					>
						<span class="font-semibold">{{
							$t('chats.forward.from', { source: forwardLabel })
						}}</span>
						<template v-if="!forwardView.deleted">
							<span v-if="message.forward.author_name">{{
								message.forward.author_name
							}}</span>
							<span v-if="forwardTime">{{ forwardTime }}</span>
						</template>
					</div>
					<p
						v-if="forwardView.deleted"
						class="mt-1 text-ink-subtle italic"
						data-testid="chat-forward-deleted"
					>
						{{ $t('chats.forward.deleted') }}
					</p>
					<template v-else>
						<p
							v-if="forwardView.edited"
							class="text-xs text-ink-subtle"
							data-testid="chat-forward-edited"
						>
							{{ $t('chats.forward.edited') }}
						</p>
						<MarkdownText
							v-if="forwardView.text"
							:content="forwardView.text"
							:untrusted="true"
						/>
						<ul v-if="forwardView.fileNames.length" class="space-y-0.5 mt-1">
							<li
								v-for="name in forwardView.fileNames"
								:key="name"
								class="inline-flex items-center gap-1 text-xs text-ink-subtle"
							>
								<Paperclip class="h-3 w-3" />
								{{ name }}
							</li>
						</ul>
						<router-link
							v-if="message.forward.task && forwardTaskLink"
							:to="forwardTaskLink"
							class="mt-1 block rounded-md border border-line bg-surface-hover p-2 hover:bg-surface-sunken dark:bg-surface"
							data-testid="chat-forward-task"
						>
							<span class="text-xs font-semibold text-brand">{{
								message.forward.task.key
							}}</span>
							<span class="ml-1 text-sm text-ink">{{
								message.forward.task.title
							}}</span>
							<span
								class="mt-0.5 flex flex-wrap gap-x-2 text-xs text-ink-subtle"
							>
								<span v-if="message.forward.task.status">{{
									message.forward.task.status.name
								}}</span>
								<span v-if="message.forward.task.assignees.length">{{
									message.forward.task.assignees.join(', ')
								}}</span>
							</span>
						</router-link>
					</template>
				</div>
				<ul v-if="message.files.length" class="space-y-1 mt-1">
					<li v-for="file in message.files" :key="file.id">
						<button
							type="button"
							class="inline-flex items-center gap-1.5 rounded-md border border-line bg-surface px-2 py-1 text-xs text-ink hover:bg-surface-hover"
							@click="download(file.id)"
						>
							<Paperclip class="h-3.5 w-3.5" />
							{{ file.name }}
						</button>
					</li>
				</ul>
				<div v-if="reactions.length" class="mt-1 flex flex-wrap gap-1">
					<button
						v-for="reaction in reactions"
						:key="reaction.emoji"
						type="button"
						class="inline-flex items-center gap-1 rounded-full border px-2 py-0.5 text-xs"
						:class="
							reaction.mine
								? 'border-brand bg-brand-bg text-ink'
								: 'border-line bg-surface text-ink-subtle hover:bg-surface-hover'
						"
						:disabled="!canWrite"
						@click="react(reaction.emoji)"
					>
						<span>{{ reaction.emoji }}</span>
						<span>{{ reaction.count }}</span>
					</button>
				</div>
				<button
					v-if="showReply && (message.reply_count > 0 || canWrite)"
					type="button"
					class="mt-1 text-xs text-brand hover:underline"
					data-testid="chat-reply-in-thread"
					@click="$emit('reply', message)"
				>
					{{
						message.reply_count > 0
							? $t('chats.replies', message.reply_count)
							: $t('chats.replyInThread')
					}}
				</button>
			</template>
		</div>
		<div
			v-if="!message.deleted && !editing && canWrite"
			class="absolute -top-3 right-2 hidden items-center gap-0.5 rounded-md border border-line bg-surface p-0.5 shadow-sm group-hover:flex"
		>
			<button
				v-for="emoji in emojis"
				:key="emoji"
				type="button"
				class="rounded px-1 text-sm hover:bg-surface-hover"
				@click="react(emoji)"
			>
				{{ emoji }}
			</button>
			<button
				v-if="showReply"
				type="button"
				class="rounded p-1 text-ink-subtle hover:bg-surface-hover hover:text-ink"
				:title="$t('chats.replyInThread')"
				:aria-label="$t('chats.replyInThread')"
				@click="$emit('reply', message)"
			>
				<MessageSquare class="h-4 w-4" />
			</button>
			<button
				v-if="sourceActions"
				type="button"
				class="rounded p-1 text-ink-subtle hover:bg-surface-hover hover:text-ink"
				:title="$t('chats.forward.action')"
				:aria-label="$t('chats.forward.action')"
				data-testid="chat-forward"
				@click="forwardOpen = true"
			>
				<Forward class="h-4 w-4" />
			</button>
			<button
				v-if="sourceActions"
				type="button"
				class="rounded p-1 text-ink-subtle hover:bg-surface-hover hover:text-ink"
				:title="$t('chats.forward.createTask')"
				:aria-label="$t('chats.forward.createTask')"
				data-testid="chat-create-task"
				@click="createTask"
			>
				<ListPlus class="h-4 w-4" />
			</button>
			<button
				v-if="mayEdit"
				type="button"
				class="rounded p-1 text-ink-subtle hover:bg-surface-hover hover:text-ink"
				:title="$t('chats.editMessage')"
				:aria-label="$t('chats.editMessage')"
				data-testid="chat-edit"
				@click="startEdit"
			>
				<Pencil class="h-4 w-4" />
			</button>
			<button
				v-if="mayDelete"
				type="button"
				class="rounded p-1 text-ink-subtle hover:bg-surface-hover hover:text-destructive"
				:title="$t('chats.deleteMessage')"
				:aria-label="$t('chats.deleteMessage')"
				data-testid="chat-delete"
				@click="confirmDelete = true"
			>
				<Trash2 class="h-4 w-4" />
			</button>
		</div>
		<ForwardDialog
			v-if="sourceActions"
			v-model:open="forwardOpen"
			:source="{ type: 'message', id: message.id }"
		/>
		<AlertDialog v-model:open="confirmDelete">
			<AlertDialogContent>
				<AlertDialogHeader>
					<AlertDialogTitle>{{
						$t('chats.deleteMessageTitle')
					}}</AlertDialogTitle>
					<AlertDialogDescription>{{
						$t('chats.deleteMessageBody')
					}}</AlertDialogDescription>
				</AlertDialogHeader>
				<AlertDialogFooter>
					<AlertDialogCancel>{{
						$t('common.actions.cancel')
					}}</AlertDialogCancel>
					<AlertDialogAction
						class="bg-destructive text-destructive-foreground hover:bg-destructive/90"
						@click="remove"
						>{{ $t('common.actions.delete') }}</AlertDialogAction
					>
				</AlertDialogFooter>
			</AlertDialogContent>
		</AlertDialog>
	</div>
</template>

<script lang="ts">
	import { getFileDownload, toggleReaction } from '@/actions/tmgr/chats';
	import type { ChatMessage, ChatReaction } from '@/actions/tmgr/chats';
	import ForwardDialog from '@/components/chats/ForwardDialog.vue';
	import MarkdownText from '@/components/general/MarkdownText.vue';
	import {
		AlertDialog,
		AlertDialogAction,
		AlertDialogCancel,
		AlertDialogContent,
		AlertDialogDescription,
		AlertDialogFooter,
		AlertDialogHeader,
		AlertDialogTitle,
	} from '@/components/ui/alert-dialog';
	import { Button } from '@/components/ui/button';
	import { toast } from '@/components/ui/toast';
	import {
		canDeleteMessage,
		canEditMessage,
		useChats,
	} from '@/composable/useChats';
	import { t } from '@/i18n';
	import {
		forwardCardView,
		forwardSnapshotLabel,
		taskDraftFromMessage,
		taskLink,
	} from '@/utils/chatForward';
	import { canStopMessage, isPersonaMessage } from '@/utils/chatPersonas';
	import { isEnterSend } from '@/utils/commentComposer';
	import { DEFAULT_REACTION_EMOJIS } from '@/utils/commentReactions';
	import {
		Forward,
		ListPlus,
		MessageSquare,
		Paperclip,
		Pencil,
		Trash2,
	} from 'lucide-vue-next';
	import { useStore } from 'vuex';
	import { computed, defineComponent, ref, watch, type PropType } from 'vue';

	export default defineComponent({
		name: 'ChatMessageItem',
		components: {
			Button,
			AlertDialog,
			AlertDialogAction,
			AlertDialogCancel,
			AlertDialogContent,
			AlertDialogDescription,
			AlertDialogFooter,
			AlertDialogHeader,
			AlertDialogTitle,
			ForwardDialog,
			Forward,
			ListPlus,
			MarkdownText,
			MessageSquare,
			Paperclip,
			Pencil,
			Trash2,
		},
		props: {
			message: { type: Object as PropType<ChatMessage>, required: true },
			canWrite: { type: Boolean, default: true },
			showReply: { type: Boolean, default: true },
			canModerate: { type: Boolean, default: false },
			canForward: { type: Boolean, default: false },
			members: {
				type: Array as PropType<
					Array<{ id: number; name: string; kind?: 'user' | 'persona' }>
				>,
				default: () => [],
			},
		},
		emits: ['reply'],
		setup(props) {
			const store = useStore();
			const { saveEdit, removeMessage, stopReply } = useChats();
			const selfId = computed<number | null>(
				() => store.state.user?.id ?? null,
			);
			const mayEdit = computed(() =>
				canEditMessage(props.message, selfId.value),
			);
			const mayDelete = computed(() =>
				canDeleteMessage(props.message, selfId.value, props.canModerate),
			);
			const sourceActions = computed(
				() =>
					props.canForward &&
					!props.message.deleted &&
					(!props.message.status || props.message.status === 'done'),
			);
			const forwardOpen = ref(false);
			const workspaceCode = computed<string>(
				() => store.getters.currentWorkspace?.code ?? '',
			);
			const forwardView = computed(() =>
				props.message.forward ? forwardCardView(props.message.forward) : null,
			);
			const forwardLabel = computed(() =>
				props.message.forward
					? forwardSnapshotLabel(props.message.forward)
					: '',
			);
			const forwardTime = computed(() =>
				props.message.forward?.created_at
					? new Date(props.message.forward.created_at).toLocaleString([], {
							dateStyle: 'short',
							timeStyle: 'short',
						})
					: '',
			);
			const forwardTaskLink = computed(() => {
				const forward = props.message.forward;
				const id =
					forward?.task_id ?? (forward?.type === 'task' ? forward.id : null);
				return id && workspaceCode.value
					? taskLink(workspaceCode.value, id)
					: null;
			});
			const createTask = () => {
				store.commit(
					'setTaskPrefill',
					taskDraftFromMessage(
						props.message.body,
						workspaceCode.value,
						props.message.chat_id,
						props.message.id,
					),
				);
				store.commit('setShowCreatingTaskModal');
			};
			const isPersona = computed(() => isPersonaMessage(props.message));
			const mayStop = computed(() =>
				canStopMessage(props.message, selfId.value),
			);
			const stop = async () => {
				try {
					await stopReply(props.message);
				} catch {
					toast({
						title: t('chats.persona.stopFailed'),
						variant: 'destructive',
					});
				}
			};
			const editing = ref(false);
			const draft = ref('');
			const confirmDelete = ref(false);

			const startEdit = () => {
				draft.value = props.message.body ?? '';
				editing.value = true;
			};
			const commitEdit = async () => {
				const body = draft.value.trim();
				if (!body) return;
				const mentions = props.members
					.filter((m) => body.includes(`@${m.name}`))
					.map((m) => ({ kind: m.kind ?? ('user' as const), id: m.id }));
				try {
					await saveEdit(props.message, body, mentions);
					editing.value = false;
				} catch {
					toast({ title: t('chats.editFailed'), variant: 'destructive' });
				}
			};
			const onEditKey = (event: KeyboardEvent) => {
				if (event.key === 'Escape') {
					editing.value = false;
				} else if (isEnterSend(event)) {
					event.preventDefault();
					void commitEdit();
				}
			};
			const remove = async () => {
				try {
					await removeMessage(props.message);
				} catch {
					toast({ title: t('chats.deleteFailed'), variant: 'destructive' });
				}
			};

			const reactions = ref<ChatReaction[]>(props.message.reactions);
			watch(
				() => props.message.reactions,
				(next) => {
					reactions.value = next;
				},
			);
			const initial = computed(() =>
				(props.message.author.name ?? '?').slice(0, 1).toUpperCase(),
			);
			const time = computed(() =>
				new Date(props.message.created_at).toLocaleString([], {
					dateStyle: 'short',
					timeStyle: 'short',
				}),
			);
			const react = async (emoji: string) => {
				reactions.value = await toggleReaction(
					props.message.chat_id,
					props.message.id,
					emoji,
				);
			};
			const download = async (fileId: number) => {
				const { download_url } = await getFileDownload(
					props.message.chat_id,
					fileId,
				);
				window.open(download_url, '_blank', 'noopener');
			};
			return {
				sourceActions,
				forwardOpen,
				forwardView,
				forwardLabel,
				forwardTime,
				forwardTaskLink,
				createTask,
				reactions,
				isPersona,
				mayStop,
				stop,
				mayEdit,
				mayDelete,
				editing,
				draft,
				confirmDelete,
				startEdit,
				onEditKey,
				remove,
				initial,
				time,
				react,
				download,
				emojis: DEFAULT_REACTION_EMOJIS,
			};
		},
	});
</script>
