<template>
	<nav class="space-y-4" data-testid="chat-list">
		<section v-for="group in groups" :key="group.key">
			<h2 class="mb-1 px-2 text-xs font-semibold text-ink-subtle uppercase">
				{{ group.label }}
			</h2>
			<ul class="space-y-0.5">
				<li v-for="chat in group.chats" :key="chat.id">
					<router-link
						:to="`/${workspaceCode}/chats/${chat.id}`"
						class="flex items-center gap-2 rounded-md px-2 py-1.5 text-sm hover:bg-surface-hover"
						:class="[
							chat.id === activeId
								? 'bg-surface-hover font-medium text-ink'
								: 'text-ink-subtle',
							chat.archived_at ? 'opacity-60' : '',
						]"
					>
						<Lock
							v-if="chat.kind === 'channel' && chat.visibility === 'members'"
							class="h-4 w-4 shrink-0"
						/>
						<Hash
							v-else-if="chat.kind === 'channel'"
							class="h-4 w-4 shrink-0"
						/>
						<User v-else class="h-4 w-4 shrink-0" />
						<span
							class="min-w-0 flex-1 truncate"
							:class="chat.unread ? 'font-semibold text-ink' : ''"
							>{{ titleOf(chat) }}</span
						>
						<BellOff v-if="muted(chat)" class="h-3.5 w-3.5 shrink-0" />
						<span
							v-if="chat.unread"
							class="rounded-full bg-brand px-1.5 text-xs font-semibold text-brand-fg"
							>{{ formatUnread(chat.unread) }}</span
						>
					</router-link>
				</li>
			</ul>
		</section>
	</nav>
</template>

<script lang="ts">
	import type { Chat } from '@/actions/tmgr/chats';
	import { chatTitle, formatUnread, isMuted } from '@/composable/useChats';
	import { BellOff, Hash, Lock, User } from 'lucide-vue-next';
	import { computed, defineComponent, type PropType } from 'vue';
	import { useI18n } from 'vue-i18n';

	const byActivity = (a: Chat, b: Chat) =>
		(b.last_message_at ?? '').localeCompare(a.last_message_at ?? '');

	export default defineComponent({
		name: 'ChatList',
		components: { BellOff, Hash, Lock, User },
		props: {
			chats: { type: Array as PropType<Chat[]>, required: true },
			activeId: { type: Number as PropType<number | null>, default: null },
			workspaceCode: { type: String, required: true },
			selfId: { type: Number as PropType<number | null>, default: null },
		},
		setup(props) {
			const { t } = useI18n();
			const groups = computed(() => {
				const mine = props.chats.filter((c) => c.joined);
				const channels = mine
					.filter((c) => c.kind === 'channel')
					.sort((a, b) => (a.title ?? '').localeCompare(b.title ?? ''));
				const dms = mine.filter((c) => c.kind !== 'channel').sort(byActivity);
				return [
					{ key: 'channels', label: t('chats.channels'), chats: channels },
					{ key: 'dms', label: t('chats.directMessages'), chats: dms },
				].filter((g) => g.chats.length);
			});
			const titleOf = (chat: Chat) => chatTitle(chat, props.selfId);
			const muted = (chat: Chat) => isMuted(chat);
			return { groups, titleOf, muted, formatUnread };
		},
	});
</script>
