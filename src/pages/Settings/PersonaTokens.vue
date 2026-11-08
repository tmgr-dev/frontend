<template>
	<div class="flex flex-col gap-3">
		<p v-if="loading" class="text-xs text-muted-foreground">
			{{ $t('settingsPages.personaTokens.loading') }}
		</p>

		<div v-else class="flex flex-col gap-2">
			<p v-if="tokens.length === 0" class="text-xs text-muted-foreground">
				{{ $t('settingsPages.personaTokens.empty') }}
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
						{{
							$t('settingsPages.personaTokens.details', {
								prefix: token.prefix,
								workspace: token.workspace_name,
								expires: formatDate(token.expires_at),
								lastUsed: token.last_used_at
									? formatDate(token.last_used_at)
									: $t('settingsPages.personaTokens.never'),
							})
						}}
					</p>
				</div>
				<Button
					v-if="!token.revoked_at"
					variant="outline"
					size="sm"
					class="shrink-0"
					@click="revoke(token)"
				>
					{{ $t('settingsPages.personaTokens.revoke') }}
				</Button>
			</div>
		</div>

		<div class="flex gap-2 border-t border-border pt-3">
			<Button size="sm" @click="showConnect = true">{{
				$t('settingsPages.personaTokens.connect')
			}}</Button>
			<Button
				v-if="activeTokens.length > 0"
				variant="outline"
				size="sm"
				@click="revokeAll"
			>
				{{ $t('settingsPages.personaTokens.revokeAll') }}
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
	import { currentLocale, t } from '@/i18n';
	import {
		computed,
		defineComponent,
		onMounted,
		ref,
		type PropType,
	} from 'vue';
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
				tokens.value.filter((token) => !token.revoked_at),
			);

			const load = async () => {
				loading.value = true;
				try {
					tokens.value = await listPersonaTokens(props.persona.id, false);
				} catch {
					toast({
						title: t('settingsPages.personaTokens.loadFailed'),
						variant: 'destructive',
					});
				} finally {
					loading.value = false;
				}
			};

			const isExpired = (token: PersonaToken) =>
				new Date(token.expires_at).getTime() < Date.now();

			const statusLabel = (token: PersonaToken) =>
				token.revoked_at
					? t('settingsPages.personaTokens.revoked')
					: isExpired(token)
						? t('settingsPages.personaTokens.expired')
						: t('settingsPages.personaTokens.active');

			const statusTone = (token: PersonaToken) =>
				token.revoked_at || isExpired(token)
					? 'bg-muted text-muted-foreground'
					: 'bg-emerald-100 text-emerald-800 dark:bg-emerald-950 dark:text-emerald-300';

			const formatDate = (date: string) =>
				new Date(date).toLocaleDateString(currentLocale());

			const revoke = async (token: PersonaToken) => {
				if (
					!window.confirm(
						t('settingsPages.personaTokens.confirmRevoke', {
							label: token.label,
						}),
					)
				)
					return;
				try {
					await revokePersonaToken(token.id, props.persona.id);
					await load();
				} catch {
					toast({
						title: t('settingsPages.personaTokens.revokeFailed'),
						variant: 'destructive',
					});
				}
			};

			const revokeAll = async () => {
				if (!window.confirm(t('settingsPages.personaTokens.confirmRevokeAll')))
					return;
				try {
					await revokeAllPersonaTokens(props.persona.id);
					await load();
				} catch {
					toast({
						title: t('settingsPages.personaTokens.revokeAllFailed'),
						variant: 'destructive',
					});
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
