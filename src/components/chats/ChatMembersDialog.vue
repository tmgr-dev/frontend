<template>
	<Dialog :open="open" @update:open="(v) => $emit('update:open', v)">
		<DialogContent class="max-w-md">
			<DialogHeader>
				<DialogTitle>{{ $t('chats.members.title') }}</DialogTitle>
			</DialogHeader>
			<ul
				class="divide-y divide-line max-h-60 overflow-y-auto rounded-card border border-line bg-surface"
			>
				<li
					v-for="member in chatMembers"
					:key="`${member.kind ?? 'user'}-${member.id ?? member.user_id}`"
					class="flex items-center gap-2 px-3 py-2 text-sm text-ink"
				>
					<span class="truncate">{{ member.name }}</span>
					<span
						v-if="member.kind === 'persona'"
						class="rounded bg-brand-bg px-1.5 py-0.5 text-[10px] font-semibold text-brand dark:bg-surface-hover dark:text-ink"
						data-testid="chat-member-ai"
						>{{ $t('chats.persona.badge') }}</span
					>
					<span
						v-if="member.run_by"
						class="truncate text-xs text-ink-subtle"
						data-testid="chat-member-run-by"
						>{{
							$t('chats.persona.agentRunByHint', { owner: member.run_by.name })
						}}</span
					>
					<span class="flex-1"></span>
					<Button
						v-if="member.kind === 'persona' && canModerate"
						variant="ghost"
						size="sm"
						data-testid="chat-persona-remove"
						@click="removePersona(member.id as number)"
						>{{ $t('common.actions.remove') }}</Button
					>
					<Button
						v-else-if="
							canModerate &&
							member.user_id !== null &&
							member.user_id !== selfId
						"
						variant="ghost"
						size="sm"
						@click="remove(member.user_id)"
						>{{ $t('common.actions.remove') }}</Button
					>
				</li>
			</ul>
			<template v-if="canModerate">
				<h3 class="text-sm font-semibold text-ink">
					{{ $t('chats.members.add') }}
				</h3>
				<MemberPicker v-model="selected" :members="candidates" />
				<DialogFooter>
					<Button :disabled="busy || !selected.length" @click="add">{{
						$t('common.actions.add')
					}}</Button>
				</DialogFooter>
			</template>
		</DialogContent>
	</Dialog>
</template>

<script lang="ts">
	import {
		addChatMembers,
		getChatMembers,
		removeChatMember,
		removeChatPersona,
	} from '@/actions/tmgr/chats';
	import type { ChatMember } from '@/actions/tmgr/chats';
	import MemberPicker from '@/components/chats/MemberPicker.vue';
	import { Button } from '@/components/ui/button';
	import { toast } from '@/components/ui/toast';
	import { t } from '@/i18n';
	import { serverMessageFrom } from '@/utils/serverError';
	import {
		Dialog,
		DialogContent,
		DialogFooter,
		DialogHeader,
		DialogTitle,
	} from '@/components/ui/dialog';
	import { computed, defineComponent, ref, watch, type PropType } from 'vue';

	export default defineComponent({
		name: 'ChatMembersDialog',
		components: {
			Button,
			Dialog,
			DialogContent,
			DialogFooter,
			DialogHeader,
			DialogTitle,
			MemberPicker,
		},
		props: {
			open: { type: Boolean, required: true },
			chatId: { type: Number, required: true },
			canModerate: { type: Boolean, default: false },
			selfId: { type: Number as PropType<number | null>, default: null },
			workspaceMembers: {
				type: Array as PropType<Array<{ id: number; name: string }>>,
				required: true,
			},
		},
		emits: ['update:open', 'changed'],
		setup(props, { emit }) {
			const chatMembers = ref<ChatMember[]>([]);
			const selected = ref<number[]>([]);
			const busy = ref(false);

			const load = async () => {
				chatMembers.value = await getChatMembers(props.chatId);
				emit('changed', chatMembers.value.length, chatMembers.value);
			};

			watch(
				() => props.open,
				(open) => {
					if (!open) return;
					selected.value = [];
					void load();
				},
			);

			const candidates = computed(() =>
				props.workspaceMembers.filter(
					(m) => !chatMembers.value.some((c) => c.user_id === m.id),
				),
			);

			const add = async () => {
				busy.value = true;
				try {
					await addChatMembers(props.chatId, selected.value);
					selected.value = [];
					await load();
				} finally {
					busy.value = false;
				}
			};

			const remove = async (userId: number) => {
				await removeChatMember(props.chatId, userId);
				await load();
			};

			const removePersona = async (personaId: number) => {
				try {
					await removeChatPersona(props.chatId, personaId);
					await load();
				} catch (e) {
					toast({
						title: serverMessageFrom(e, t('chats.persona.removeFailed')),
						variant: 'destructive',
					});
				}
			};

			return {
				chatMembers,
				selected,
				busy,
				candidates,
				add,
				remove,
				removePersona,
			};
		},
	});
</script>
