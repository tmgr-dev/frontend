<template>
	<Dialog :open="open" @update:open="(v) => $emit('update:open', v)">
		<DialogContent class="max-w-md">
			<DialogHeader>
				<DialogTitle>{{ $t('chats.forward.title') }}</DialogTitle>
			</DialogHeader>
			<label class="block text-xs font-medium text-ink-subtle">
				{{ $t('chats.forward.chat') }}
				<select
					v-model="targetId"
					class="mt-1 w-full rounded-md border border-line bg-surface p-2 text-sm text-ink dark:bg-surface"
					data-testid="chat-forward-target"
				>
					<option v-for="c in targets" :key="c.id" :value="c.id">
						{{ c.label }}
					</option>
				</select>
			</label>
			<p
				v-if="loaded && !targets.length"
				class="text-sm text-ink-subtle"
				data-testid="chat-forward-no-chats"
			>
				{{ $t('chats.forward.noChats') }}
			</p>
			<textarea
				v-model="note"
				rows="2"
				class="w-full resize-none rounded-md border border-line bg-surface p-2 text-sm text-ink outline-hidden placeholder:text-ink-subtle"
				:placeholder="$t('chats.forward.note')"
				data-testid="chat-forward-note"
			></textarea>
			<label
				v-if="personas.length"
				class="block text-xs font-medium text-ink-subtle"
			>
				{{ $t('chats.forward.askPersona') }}
				<select
					v-model="personaId"
					class="mt-1 w-full rounded-md border border-line bg-surface p-2 text-sm text-ink dark:bg-surface"
					data-testid="chat-forward-persona"
				>
					<option :value="null">{{ $t('chats.forward.noPersona') }}</option>
					<option v-for="p in personas" :key="p.id" :value="p.id">
						{{ p.name }}
					</option>
				</select>
			</label>
			<p
				v-if="error"
				class="text-xs text-destructive"
				data-testid="chat-forward-error"
			>
				{{ error }}
			</p>
			<DialogFooter>
				<Button variant="outline" @click="$emit('update:open', false)">{{
					$t('common.actions.cancel')
				}}</Button>
				<Button :disabled="busy || !targetId" @click="send">{{
					$t('chats.forward.send')
				}}</Button>
			</DialogFooter>
		</DialogContent>
	</Dialog>
</template>

<script lang="ts">
	import {
		forwardToChat,
		getChatMembers,
		listChats,
		type ChatMember,
		type ForwardSource,
	} from '@/actions/tmgr/chats';
	import { Button } from '@/components/ui/button';
	import {
		Dialog,
		DialogContent,
		DialogFooter,
		DialogHeader,
		DialogTitle,
	} from '@/components/ui/dialog';
	import { toast } from '@/components/ui/toast';
	import { chatTitle } from '@/composable/useChats';
	import { t } from '@/i18n';
	import { forwardErrorKey } from '@/utils/chatForward';
	import { personaMembers } from '@/utils/chatPersonas';
	import { defineComponent, ref, watch, type PropType } from 'vue';
	import { useStore } from 'vuex';

	interface Target {
		id: number;
		label: string;
	}

	export default defineComponent({
		name: 'ForwardDialog',
		components: {
			Button,
			Dialog,
			DialogContent,
			DialogFooter,
			DialogHeader,
			DialogTitle,
		},
		props: {
			open: { type: Boolean, required: true },
			source: { type: Object as PropType<ForwardSource>, required: true },
		},
		emits: ['update:open', 'forwarded'],
		setup(props, { emit }) {
			const store = useStore();
			const targets = ref<Target[]>([]);
			const targetId = ref<number | null>(null);
			const personas = ref<ChatMember[]>([]);
			const personaId = ref<number | null>(null);
			const note = ref('');
			const busy = ref(false);
			const error = ref('');
			const loaded = ref(false);

			watch(
				() => props.open,
				async (open) => {
					if (!open) return;
					targets.value = [];
					targetId.value = null;
					personas.value = [];
					personaId.value = null;
					note.value = '';
					error.value = '';
					loaded.value = false;
					const workspace = store.getters.currentWorkspace;
					if (!workspace) return;
					const selfId = store.state.user?.id ?? null;
					try {
						const chats = await listChats(workspace.id);
						targets.value = chats
							.filter((c) => c.can_write && !c.archived_at)
							.map((c) => ({ id: c.id, label: chatTitle(c, selfId) }));
					} catch {
						targets.value = [];
					}
					loaded.value = true;
					targetId.value = targets.value[0]?.id ?? null;
				},
			);

			watch(targetId, async (id) => {
				personas.value = [];
				personaId.value = null;
				if (!id) return;
				try {
					const members = personaMembers(await getChatMembers(id));
					if (targetId.value === id) personas.value = members;
				} catch {
					personas.value = [];
				}
			});

			const send = async () => {
				if (!targetId.value) return;
				const persona = personas.value.find((p) => p.id === personaId.value);
				const body = [persona ? `@${persona.name}` : '', note.value.trim()]
					.filter(Boolean)
					.join(' ');
				busy.value = true;
				error.value = '';
				try {
					await forwardToChat(targetId.value, props.source, {
						body,
						mentions: persona
							? [{ kind: 'persona', id: persona.id as number }]
							: [],
					});
					toast({ title: t('chats.forward.sent') });
					emit('forwarded');
					emit('update:open', false);
				} catch (e) {
					error.value = t(forwardErrorKey(e));
				} finally {
					busy.value = false;
				}
			};

			return {
				targets,
				targetId,
				personas,
				personaId,
				note,
				busy,
				error,
				loaded,
				send,
			};
		},
	});
</script>
