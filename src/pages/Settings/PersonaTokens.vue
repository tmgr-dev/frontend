<template>
	<div class="flex flex-col gap-3">
		<p v-if="loading" class="text-xs text-muted-foreground">
			Loading agent connections…
		</p>

		<div v-else class="flex flex-col gap-2">
			<p v-if="tokens.length === 0" class="text-xs text-muted-foreground">
				No agent connections yet.
			</p>
			<div
				v-for="token in tokens"
				:key="token.id"
				class="flex items-center justify-between gap-3 rounded border border-border p-2"
			>
				<div class="min-w-0">
					<p class="flex items-center gap-2 truncate text-sm font-medium">
						{{ token.label }}
						<span
							:class="[
								'rounded px-1.5 py-0.5 text-2xs font-semibold uppercase',
								statusTone(token),
							]"
						>
							{{ statusLabel(token) }}
						</span>
					</p>
					<p class="truncate text-xs text-muted-foreground">
						{{ token.prefix }}… · {{ token.workspace_name }} · expires
						{{ formatDate(token.expires_at) }} · last used
						{{ token.last_used_at ? formatDate(token.last_used_at) : 'never' }}
					</p>
				</div>
				<Button
					v-if="!token.revoked_at"
					variant="outline"
					size="sm"
					class="shrink-0"
					@click="revoke(token)"
				>
					Revoke
				</Button>
			</div>
		</div>

		<div class="flex gap-2 border-t border-border pt-3">
			<Button size="sm" @click="showConnect = true">Connect an agent</Button>
			<Button
				v-if="activeTokens.length > 0"
				variant="outline"
				size="sm"
				@click="revokeAll"
			>
				Revoke all
			</Button>
		</div>

		<PersonaConnectDialog
			v-model:open="showConnect"
			:persona="persona"
			@issued="load"
		/>
	</div>
</template>

<script lang="ts">
	import {
		listPersonaTokens,
		revokeAllPersonaTokens,
		revokePersonaToken,
		type Persona,
		type PersonaToken,
	} from '@/actions/tmgr/personas';
	import { Button } from '@/components/ui/button';
	import { toast } from '@/components/ui/toast';
	import { computed, defineComponent, onMounted, ref, type PropType } from 'vue';
	import PersonaConnectDialog from './PersonaConnectDialog.vue';

	export default defineComponent({
		name: 'PersonaTokens',
		components: { Button, PersonaConnectDialog },
		props: {
			persona: { type: Object as PropType<Persona>, required: true },
		},
		setup(props) {
			const tokens = ref<PersonaToken[]>([]);
			const loading = ref(true);
			const showConnect = ref(false);

			const activeTokens = computed(() =>
				tokens.value.filter((t) => !t.revoked_at),
			);

			const load = async () => {
				loading.value = true;
				try {
					tokens.value = await listPersonaTokens(props.persona.id, false);
				} catch {
					toast({
						title: 'Could not load agent connections',
						variant: 'destructive',
					});
				} finally {
					loading.value = false;
				}
			};

			const isExpired = (token: PersonaToken) =>
				new Date(token.expires_at).getTime() < Date.now();

			const statusLabel = (token: PersonaToken) =>
				token.revoked_at ? 'Revoked' : isExpired(token) ? 'Expired' : 'Active';

			const statusTone = (token: PersonaToken) =>
				token.revoked_at || isExpired(token)
					? 'bg-muted text-muted-foreground'
					: 'bg-emerald-100 text-emerald-800 dark:bg-emerald-950 dark:text-emerald-300';

			const formatDate = (date: string) => new Date(date).toLocaleDateString();

			const revoke = async (token: PersonaToken) => {
				if (!window.confirm(`Revoke the "${token.label}" connection?`)) return;
				try {
					await revokePersonaToken(token.id, props.persona.id);
					await load();
				} catch {
					toast({ title: 'Could not revoke token', variant: 'destructive' });
				}
			};

			const revokeAll = async () => {
				if (
					!window.confirm(
						'Revoke every agent connection for this persona?',
					)
				)
					return;
				try {
					await revokeAllPersonaTokens(props.persona.id);
					await load();
				} catch {
					toast({ title: 'Could not revoke tokens', variant: 'destructive' });
				}
			};

			onMounted(load);

			return {
				tokens,
				loading,
				showConnect,
				activeTokens,
				load,
				statusLabel,
				statusTone,
				formatDate,
				revoke,
				revokeAll,
			};
		},
	});
</script>
