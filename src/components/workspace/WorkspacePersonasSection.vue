<template>
	<SettingsSection
		title="Personas"
		description="AI personas with access to this workspace."
	>
		<div v-if="isCreator" class="mb-4 flex flex-col gap-1.5">
			<Label for="persona-policy">Policy</Label>
			<Select
				:model-value="policyDraft"
				:disabled="savingPolicy"
				@update:model-value="onPolicyChange"
			>
				<SelectTrigger id="persona-policy" class="w-48">
					<SelectValue />
				</SelectTrigger>
				<SelectContent>
					<SelectItem value="allowed">Allowed</SelectItem>
					<SelectItem value="read_only">Read only</SelectItem>
					<SelectItem value="forbidden">Forbidden</SelectItem>
				</SelectContent>
			</Select>
		</div>
		<p v-else-if="policy" class="mb-4 text-sm text-ink-subtle">
			Policy: {{ policyLabel }}
		</p>

		<div v-if="loading" class="text-sm text-muted-foreground">
			Loading personas…
		</div>
		<div v-else-if="grants.length === 0" class="text-sm text-muted-foreground">
			No personas have access to this workspace yet.
		</div>
		<div v-else class="flex flex-col gap-2">
			<div
				v-for="grant in grants"
				:key="grant.persona.id"
				class="flex items-center justify-between gap-3 rounded-md border border-border p-3"
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
							<span class="text-xs font-normal text-muted-foreground">
								persona of {{ grant.persona.owner.name }}
							</span>
						</p>
						<p class="text-xs text-muted-foreground">
							Permissions: {{ grant.permissions.join(', ') || 'none' }} ·
							Effective: {{ grant.effective_permissions.join(', ') || 'none' }}
						</p>
						<p v-if="grant.blocked" class="text-xs font-medium text-destructive">
							Blocked
						</p>
						<p
							v-if="grant.persona.skills?.length"
							class="text-xs text-muted-foreground"
						>
							Skills: {{ grant.persona.skills.map((s) => s.title).join(', ') }}
						</p>
					</div>
				</div>
				<Button
					v-if="isCreator"
					type="button"
					variant="outline"
					size="sm"
					:disabled="!!blocking[grant.persona.id]"
					@click="toggleBlock(grant)"
				>
					{{ grant.blocked ? 'Unblock' : 'Block' }}
				</Button>
			</div>
		</div>
	</SettingsSection>
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
	import SettingsSection from '@/components/layouts/SettingsSection.vue';
	import { Button } from '@/components/ui/button';
	import { Label } from '@/components/ui/label';
	import {
		Select,
		SelectContent,
		SelectItem,
		SelectTrigger,
		SelectValue,
	} from '@/components/ui/select';
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
		components: {
			Button,
			Label,
			PersonaAvatar,
			Select,
			SelectContent,
			SelectItem,
			SelectTrigger,
			SelectValue,
			SettingsSection,
		},
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

			const onPolicyChange = (value: string) => {
				policyDraft.value = value as PersonaPolicy;
				savePolicy();
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
				onPolicyChange,
				toggleBlock,
			};
		},
	});
</script>
