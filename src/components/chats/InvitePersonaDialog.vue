<template>
	<Dialog :open="open" @update:open="(v) => $emit('update:open', v)">
		<DialogContent class="max-w-md">
			<DialogHeader>
				<DialogTitle>{{ $t('chats.persona.invite') }}</DialogTitle>
			</DialogHeader>
			<ul
				class="divide-y divide-line max-h-60 overflow-y-auto rounded-card border border-line bg-surface"
				data-testid="chat-invite-persona-list"
			>
				<li v-for="persona in personas" :key="persona.id">
					<label
						class="flex cursor-pointer items-center gap-2 px-3 py-2 text-sm text-ink hover:bg-surface-hover"
					>
						<input
							v-model="selected"
							type="radio"
							name="chat-persona"
							:value="persona.id"
						/>
						{{ persona.name }}
					</label>
				</li>
				<li v-if="!personas.length" class="px-3 py-2 text-sm text-ink-subtle">
					{{ $t('chats.persona.inviteNone') }}
				</li>
			</ul>
			<p
				v-if="error"
				class="text-xs text-destructive"
				data-testid="chat-invite-error"
			>
				{{ error }}
			</p>
			<DialogFooter>
				<Button :disabled="busy || !selected" @click="invite">{{
					$t('chats.persona.invite')
				}}</Button>
			</DialogFooter>
		</DialogContent>
	</Dialog>
</template>

<script lang="ts">
	import { invitePersona } from '@/actions/tmgr/chats';
	import { getAssignablePersonas } from '@/actions/tmgr/personas';
	import { Button } from '@/components/ui/button';
	import {
		Dialog,
		DialogContent,
		DialogFooter,
		DialogHeader,
		DialogTitle,
	} from '@/components/ui/dialog';
	import { toast } from '@/components/ui/toast';
	import { t } from '@/i18n';
	import { serverMessageFrom } from '@/utils/serverError';
	import type { PersonaAssignee } from '@/utils/personas';
	import { defineComponent, ref, watch } from 'vue';
	import { useStore } from 'vuex';

	export default defineComponent({
		name: 'InvitePersonaDialog',
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
			chatId: { type: Number, required: true },
		},
		emits: ['update:open', 'invited'],
		setup(props, { emit }) {
			const store = useStore();
			const personas = ref<PersonaAssignee[]>([]);
			const selected = ref<string | null>(null);
			const busy = ref(false);
			const error = ref('');

			watch(
				() => props.open,
				async (open) => {
					if (!open) return;
					selected.value = null;
					error.value = '';
					const workspace = store.getters.currentWorkspace;
					if (!workspace) return;
					try {
						personas.value = await getAssignablePersonas(workspace.id);
					} catch {
						personas.value = [];
					}
				},
			);

			const invite = async () => {
				if (!selected.value) return;
				busy.value = true;
				error.value = '';
				try {
					await invitePersona(props.chatId, selected.value);
					toast({ title: t('chats.persona.invited') });
					emit('invited');
					emit('update:open', false);
				} catch (e) {
					error.value = serverMessageFrom(e, t('chats.persona.inviteFailed'));
				} finally {
					busy.value = false;
				}
			};

			return { personas, selected, busy, error, invite };
		},
	});
</script>
