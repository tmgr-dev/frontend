<template>
	<div
		class="rounded-lg border border-gray-200 bg-white p-6 dark:border-gray-700 dark:bg-gray-800"
	>
		<h3 class="mb-1 text-xl font-semibold">Personas</h3>
		<p class="mb-4 text-sm text-gray-600 dark:text-gray-400">
			AI personas with access to this workspace.
		</p>

		<div v-if="isCreator" class="mb-4 flex items-center gap-2">
			<label class="text-sm font-medium" for="persona-policy">Policy</label>
			<select
				id="persona-policy"
				v-model="policyDraft"
				:disabled="savingPolicy"
				class="rounded-md border border-gray-300 bg-white px-2 py-1 text-sm dark:border-gray-600 dark:bg-gray-700"
				@change="savePolicy"
			>
				<option value="allowed">Allowed</option>
				<option value="read_only">Read only</option>
				<option value="forbidden">Forbidden</option>
			</select>
		</div>
		<p v-else-if="policy" class="mb-4 text-sm text-gray-600 dark:text-gray-400">
			Policy: {{ policyLabel }}
		</p>

		<div v-if="loading" class="text-sm text-gray-500">Loading personas…</div>
		<div v-else-if="grants.length === 0" class="text-sm text-gray-500">
			No personas have access to this workspace yet.
		</div>
		<div v-else class="flex flex-col gap-2">
			<div
				v-for="grant in grants"
				:key="grant.persona.id"
				class="flex items-center justify-between gap-3 rounded-md border border-gray-200 p-3 dark:border-gray-700"
			>
				<div class="flex items-center gap-2">
					<PersonaAvatar
						:uuid="grant.persona.id"
						:name="grant.persona.name"
						:has-avatar="!!grant.persona.avatar_url"
						:size="28"
					/>
					<div>
						<p class="text-sm font-medium">
							{{ grant.persona.name }}
							<span class="text-xs font-normal text-gray-500">
								persona of {{ grant.persona.owner.name }}
							</span>
						</p>
						<p class="text-xs text-gray-500">
							Permissions: {{ grant.permissions.join(', ') || 'none' }} ·
							Effective: {{ grant.effective_permissions.join(', ') || 'none' }}
						</p>
						<p
							v-if="grant.blocked"
							class="text-xs font-medium text-red-600 dark:text-red-400"
						>
							Blocked
						</p>
					</div>
				</div>
				<button
					v-if="isCreator"
					type="button"
					:disabled="!!blocking[grant.persona.id]"
					class="rounded-md border border-gray-300 px-2 py-1 text-xs font-medium hover:bg-gray-50 dark:border-gray-600 dark:hover:bg-gray-700"
					@click="toggleBlock(grant)"
				>
					{{ grant.blocked ? 'Unblock' : 'Block' }}
				</button>
			</div>
		</div>
	</div>
</template>

<script lang="ts">
	import {
		blockWorkspacePersona,
		getPersonaPolicy,
		listWorkspacePersonas,
		type PersonaGrant,
		type PersonaPolicyResponse,
		setPersonaPolicy,
		unblockWorkspacePersona,
	} from '@/actions/tmgr/personas';
	import PersonaAvatar from '@/components/general/PersonaAvatar.vue';
	import { toast } from '@/components/ui/toast';
	import type { PersonaPolicy } from '@/utils/personas';
	import { computed, defineComponent, onMounted, reactive, ref, watch } from 'vue';

	const POLICY_LABELS: Record<PersonaPolicy, string> = {
		allowed: 'Allowed',
		read_only: 'Read only',
		forbidden: 'Forbidden',
	};

	export default defineComponent({
		name: 'WorkspacePersonasSection',
		components: { PersonaAvatar },
		props: {
			workspaceId: { type: Number, required: true },
			isCreator: { type: Boolean, default: false },
		},
		setup(props) {
			const grants = ref<PersonaGrant[]>([]);
			const policy = ref<PersonaPolicyResponse | null>(null);
			const policyDraft = ref<PersonaPolicy>('allowed');
			const loading = ref(true);
			const savingPolicy = ref(false);
			const blocking = reactive<Record<string, boolean>>({});

			const load = async () => {
				loading.value = true;
				try {
					const [g, p] = await Promise.all([
						listWorkspacePersonas(props.workspaceId),
						getPersonaPolicy(props.workspaceId).catch(() => null),
					]);
					grants.value = g;
					policy.value = p;
					if (p) policyDraft.value = p.policy;
				} finally {
					loading.value = false;
				}
			};

			const savePolicy = async () => {
				savingPolicy.value = true;
				try {
					policy.value = await setPersonaPolicy(
						props.workspaceId,
						policyDraft.value,
					);
					toast({ title: 'Persona policy updated' });
				} catch {
					toast({ title: 'Could not update policy', variant: 'destructive' });
					if (policy.value) policyDraft.value = policy.value.policy;
				} finally {
					savingPolicy.value = false;
				}
			};

			const toggleBlock = async (grant: PersonaGrant) => {
				blocking[grant.persona.id] = true;
				try {
					const updated = grant.blocked
						? await unblockWorkspacePersona(props.workspaceId, grant.persona.id)
						: await blockWorkspacePersona(props.workspaceId, grant.persona.id);
					const index = grants.value.findIndex(
						(g) => g.persona.id === grant.persona.id,
					);
					if (index !== -1) grants.value[index] = updated;
				} catch {
					toast({ title: 'Could not change block state', variant: 'destructive' });
				} finally {
					blocking[grant.persona.id] = false;
				}
			};

			onMounted(load);
			watch(() => props.workspaceId, load);

			return {
				grants,
				policy,
				policyDraft,
				policyLabel: computed(
					() => (policy.value ? POLICY_LABELS[policy.value.policy] : ''),
				),
				loading,
				savingPolicy,
				blocking,
				savePolicy,
				toggleBlock,
			};
		},
	});
</script>
