<template>
	<div class="flex h-full min-h-0 flex-1" data-testid="chat-view">
		<p v-if="!chat" class="p-6 text-sm text-ink-subtle">
			{{ loading ? $t('common.states.loading') : $t('chats.notFound') }}
		</p>
		<div v-else class="flex min-h-0 min-w-0 flex-1">
			<section class="flex min-h-0 min-w-0 flex-1 flex-col">
				<header class="flex items-center gap-3 border-b border-line px-4 py-3">
					<div class="min-w-0 flex-1">
						<h1
							class="flex items-center gap-1 truncate text-base font-semibold text-ink"
						>
							<Lock
								v-if="chat.kind === 'channel' && chat.visibility === 'members'"
								class="h-4 w-4 shrink-0"
							/>
							<span class="truncate"
								>{{
									chat.kind === 'channel' && chat.visibility !== 'members'
										? '# '
										: ''
								}}{{ title }}</span
							>
						</h1>
						<p
							v-if="chat.can_read && chat.topic"
							class="truncate text-xs text-ink-subtle"
						>
							{{ chat.topic }}
						</p>
					</div>
					<Button
						v-if="chat.can_read"
						variant="ghost"
						size="sm"
						data-testid="chat-members"
						@click="showMembers = true"
					>
						<Users />
						{{ chat.member_count }}
					</Button>
					<ChatMenu
						:chat="chat"
						@mark-read="markOpenChatRead"
						@gone="leaveView"
					/>
				</header>

				<div
					v-if="!chat.can_read"
					class="flex flex-1 flex-col items-center justify-center gap-1 p-6 text-center"
				>
					<Lock class="h-8 w-8 text-ink-subtle" />
					<p class="text-sm text-ink">{{ title }}</p>
					<p class="text-sm text-ink-subtle">
						{{ $t('chats.memberCount', chat.member_count) }}
					</p>
					<p class="text-sm text-ink-subtle">{{ $t('chats.closedChannel') }}</p>
				</div>
				<template v-else>
					<div
						ref="scroller"
						class="space-y-0.5 min-h-0 flex-1 overflow-y-auto p-3"
						data-testid="chat-messages"
						@scroll="onScroll"
					>
						<p v-if="loading" class="text-sm text-ink-subtle">
							{{ $t('common.states.loading') }}
						</p>
						<p
							v-else-if="!messages.length"
							class="py-10 text-center text-sm text-ink-subtle"
						>
							{{ $t('chats.empty') }}
						</p>
						<ChatMessageItem
							v-for="message in messages"
							:key="message.id"
							:message="message"
							:can-write="chat.can_write"
							:can-moderate="chat.can_moderate"
							:members="members"
							@reply="openThread"
						/>
					</div>
					<div class="p-3">
						<div
							v-if="!chat.joined && chat.visibility === 'workspace'"
							class="flex items-center justify-between rounded-card border border-line bg-surface px-3 py-2"
						>
							<span class="text-sm text-ink-subtle">{{
								$t('chats.notJoined')
							}}</span>
							<Button size="sm" @click="join">{{ $t('chats.join') }}</Button>
						</div>
						<ChatComposer
							v-else
							:key="chat.id"
							:members="members"
							:disabled="!chat.can_write"
							:disabled-hint="
								chat.archived_at
									? $t('chats.archivedHint')
									: $t('chats.readOnly')
							"
							:placeholder="$t('chats.messagePlaceholder')"
							:submit-message="submit"
						/>
					</div>
				</template>
			</section>
			<ThreadPanel
				v-if="threadRoot && chat.can_read"
				:root="threadRoot"
				:replies="replies"
				:can-write="chat.can_write"
				:can-moderate="chat.can_moderate"
				:members="members"
				@close="closeThread"
			/>
		</div>

		<ChatMembersDialog
			v-if="chat"
			v-model:open="showMembers"
			:chat-id="chat.id"
			:can-moderate="chat.can_moderate"
			:self-id="selfId"
			:workspace-members="members"
			@changed="onMembersChanged"
		/>
	</div>
</template>

<script lang="ts">
	import { joinChat } from '@/actions/tmgr/chats';
	import type { ChatMessage, SendMessagePayload } from '@/actions/tmgr/chats';
	import { getWorkspaceMembers } from '@/actions/tmgr/workspaces';
	import ChatComposer from '@/components/chats/ChatComposer.vue';
	import ChatMembersDialog from '@/components/chats/ChatMembersDialog.vue';
	import ChatMenu from '@/components/chats/ChatMenu.vue';
	import ChatMessageItem from '@/components/chats/ChatMessageItem.vue';
	import ThreadPanel from '@/components/chats/ThreadPanel.vue';
	import { Button } from '@/components/ui/button';
	import { chatTitle, useChats } from '@/composable/useChats';
	import { Lock, Users } from 'lucide-vue-next';
	import { computed, defineComponent, nextTick, ref, watch } from 'vue';
	import { useRoute, useRouter } from 'vue-router';
	import { useStore } from 'vuex';

	export default defineComponent({
		name: 'ChatView',
		components: {
			Button,
			ChatComposer,
			ChatMembersDialog,
			ChatMenu,
			ChatMessageItem,
			Lock,
			ThreadPanel,
			Users,
		},
		setup() {
			const route = useRoute();
			const router = useRouter();
			const store = useStore();
			const {
				chats,
				messages,
				hasOlder,
				loadingMessages,
				threadRoot,
				replies,
				openChat,
				closeChat,
				loadOlder,
				openThread,
				closeThread,
				send,
				upsertChat,
				scheduleMarkRead,
				markOpenChatRead,
			} = useChats();

			const scroller = ref<HTMLElement | null>(null);
			const showMembers = ref(false);
			const members = ref<Array<{ id: number; name: string }>>([]);

			const chatId = computed(() => Number(route.params.chat_id));
			const chat = computed(
				() => chats.value.find((c) => c.id === chatId.value) ?? null,
			);
			const selfId = computed<number | null>(
				() => store.state.user?.id ?? null,
			);
			const title = computed(() =>
				chat.value ? chatTitle(chat.value, selfId.value) : '',
			);
			const loading = computed(() => loadingMessages.value);

			const scrollToBottom = async () => {
				await nextTick();
				const el = scroller.value;
				if (el) el.scrollTop = el.scrollHeight;
			};

			const load = async () => {
				try {
					await openChat(chatId.value);
				} catch {
					return;
				}
				await scrollToBottom();
				scheduleMarkRead();
			};

			const loadMembers = async () => {
				const workspace = store.getters.currentWorkspace;
				if (!workspace) return;
				const all = await getWorkspaceMembers(workspace.id);
				members.value = all
					.filter((m) => m.id !== selfId.value)
					.map((m) => ({ id: m.id, name: m.name }));
			};

			watch(chatId, load, { immediate: true });
			void loadMembers().catch(() => undefined);

			watch(
				() => messages.value.length,
				async (next, prev) => {
					const el = scroller.value;
					const last = messages.value[messages.value.length - 1];
					const nearBottom =
						!!el && el.scrollHeight - el.scrollTop - el.clientHeight < 120;
					const mine = last?.author.id === selfId.value;
					if (prev === 0 || next > prev) {
						if (nearBottom || mine || prev === 0) await scrollToBottom();
						scheduleMarkRead();
					}
				},
			);

			watch(
				() => chats.value.some((c) => c.id === chatId.value),
				(present, wasPresent) => {
					if (wasPresent && !present && !loadingMessages.value) leaveView();
				},
			);

			const onScroll = async () => {
				const el = scroller.value;
				if (!el || el.scrollTop > 80 || !hasOlder.value) return;
				const before = el.scrollHeight;
				await loadOlder();
				await nextTick();
				el.scrollTop = el.scrollHeight - before;
			};

			const submit = (
				payload: Pick<SendMessagePayload, 'body' | 'mentions' | 'files'>,
			) => send(chatId.value, payload);

			const join = async () => upsertChat(await joinChat(chatId.value));

			const leaveView = () => {
				closeChat();
				router.push(`/${store.getters.currentWorkspace.code}/chats`);
			};

			const onMembersChanged = (count: number) => {
				if (chat.value) upsertChat({ ...chat.value, member_count: count });
			};

			watch(
				() => route.params.chat_id,
				(next, prev) => {
					if (prev && !next) closeChat();
				},
			);

			return {
				chat,
				title,
				loading,
				scroller,
				messages,
				members,
				selfId,
				threadRoot,
				replies,
				showMembers,
				onScroll,
				openThread,
				closeThread,
				submit,
				join,
				leaveView,
				markOpenChatRead,
				onMembersChanged,
			};
		},
	});
</script>
