<template>
	<aside
		class="flex h-full min-h-0 w-full flex-col border-l border-line bg-surface md:w-96"
		data-testid="chat-thread"
	>
		<header
			class="flex items-center justify-between border-b border-line px-4 py-3"
		>
			<h2 class="text-sm font-semibold text-ink">{{ $t('chats.thread') }}</h2>
			<button
				type="button"
				class="rounded p-1 text-ink-subtle hover:bg-surface-hover hover:text-ink"
				:aria-label="$t('common.actions.close')"
				@click="$emit('close')"
			>
				<X class="h-4 w-4" />
			</button>
		</header>
		<div class="space-y-1 min-h-0 flex-1 overflow-y-auto p-2">
			<ChatMessageItem
				:message="root"
				:can-write="canWrite"
				:can-moderate="canModerate"
				:members="members"
				:show-reply="false"
			/>
			<hr class="my-2 border-line" />
			<ChatMessageItem
				v-for="reply in replies"
				:key="reply.id"
				:message="reply"
				:can-write="canWrite"
				:can-moderate="canModerate"
				:members="members"
				:show-reply="false"
			/>
		</div>
		<div class="border-t border-line p-3">
			<ChatComposer
				:key="root.id"
				:members="members"
				:disabled="!canWrite"
				:disabled-hint="$t('chats.readOnly')"
				:placeholder="$t('chats.replyPlaceholder')"
				:submit-message="submitReply"
			/>
		</div>
	</aside>
</template>

<script lang="ts">
	import type { ChatMessage, SendMessagePayload } from '@/actions/tmgr/chats';
	import ChatComposer from '@/components/chats/ChatComposer.vue';
	import ChatMessageItem from '@/components/chats/ChatMessageItem.vue';
	import { useChats } from '@/composable/useChats';
	import { reportPersonaRefusals } from '@/utils/chatPersonaToasts';
	import { X } from 'lucide-vue-next';
	import { defineComponent, type PropType } from 'vue';

	export default defineComponent({
		name: 'ThreadPanel',
		components: { ChatComposer, ChatMessageItem, X },
		props: {
			root: { type: Object as PropType<ChatMessage>, required: true },
			replies: { type: Array as PropType<ChatMessage[]>, required: true },
			canWrite: { type: Boolean, default: true },
			canModerate: { type: Boolean, default: false },
			members: {
				type: Array as PropType<
					Array<{ id: number; name: string; kind?: 'user' | 'persona' }>
				>,
				default: () => [],
			},
		},
		emits: ['close'],
		setup(props) {
			const { send } = useChats();
			const submitReply = async (
				payload: Pick<SendMessagePayload, 'body' | 'mentions' | 'files'>,
			) =>
				reportPersonaRefusals(
					await send(props.root.chat_id, {
						...payload,
						parent_message_id: props.root.id,
					}),
				);
			return { submitReply };
		},
	});
</script>
