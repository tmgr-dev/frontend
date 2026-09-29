<template>
	<PageContainer width="narrow">
		<PageHeader title="Agent notifications">
			<template #subtitle>
				Get a push notification on your phone when Claude Code, Codex or
				another AI agent needs you — the recipient is always you, the token
				owner, and a token works across every workspace you're in.
			</template>
		</PageHeader>

		<SettingsSection title="New token" class="mb-6">
			<form class="flex flex-col gap-3" @submit.prevent="submitCreate">
				<label class="flex flex-col gap-1">
					<span class="text-sm font-medium">Label (optional)</span>
					<Input
						:model-value="label"
						maxlength="60"
						placeholder="Claude Code on my laptop"
						@update:model-value="(v) => (label = String(v))"
					/>
				</label>
				<div>
					<Button type="submit" :disabled="creating">
						{{ creating ? 'Creating…' : 'Create token' }}
					</Button>
				</div>
			</form>

			<div v-if="issuedToken" class="mt-4 flex flex-col gap-2 border-t border-border pt-4">
				<p class="text-sm font-medium text-amber-600 dark:text-amber-400">
					Copy this token now — you won't see it again.
				</p>
				<div class="flex items-center gap-2">
					<code
						class="min-w-0 flex-1 overflow-x-auto rounded bg-muted px-2 py-1.5 font-mono text-xs"
					>
						{{ issuedToken.token }}
					</code>
					<Button type="button" variant="outline" size="sm" @click="copyToken">
						{{ copied ? 'Copied' : 'Copy' }}
					</Button>
				</div>
				<Button type="button" size="sm" class="w-fit" @click="issuedToken = null">
					Done
				</Button>
			</div>
		</SettingsSection>

		<p v-if="loadError" class="mb-4 text-sm text-destructive">
			{{ loadError }}
		</p>

		<SettingsSection title="Tokens">
			<p v-if="loading" class="text-sm text-muted-foreground">
				Loading tokens…
			</p>
			<div v-else class="flex flex-col gap-2">
				<p v-if="tokens.length === 0" class="text-sm text-muted-foreground">
					No agent notification tokens yet.
				</p>
				<div
					v-for="token in tokens"
					:key="token.id"
					class="flex items-center justify-between gap-3 rounded border border-border p-2"
				>
					<div class="min-w-0">
						<p class="truncate text-sm font-medium">
							{{ token.label || 'Untitled token' }}
						</p>
						<p class="truncate text-xs text-muted-foreground">
							{{ token.prefix }}… · created {{ formatDate(token.created_at) }}
							· last used
							{{
								token.last_used_at ? formatDate(token.last_used_at) : 'never'
							}}
						</p>
					</div>
					<Button
						variant="outline"
						size="sm"
						class="shrink-0"
						@click="revoke(token)"
					>
						Revoke
					</Button>
				</div>
			</div>
		</SettingsSection>

		<SettingsSection title="Setup" class="mt-6">
			<div class="flex flex-col gap-2 text-sm text-muted-foreground">
				<p>Configure your agent with:</p>
				<pre
					class="overflow-x-auto whitespace-pre rounded bg-muted p-3 font-mono text-xs text-ink"
					>TMGR_URL={{ apiUrl }}
TMGR_NOTIFY_TOKEN=&lt;paste a token from above&gt;</pre
				>
				<p>
					See <code class="rounded bg-muted px-1 py-0.5 font-mono text-xs">tools/tmgr-notify</code>
					in the backend repo for the notify CLI and its README.
				</p>
			</div>
		</SettingsSection>
	</PageContainer>
</template>

<script lang="ts">
	import {
		createNotifyToken,
		listNotifyTokens,
		revokeNotifyToken,
		type IssuedNotifyToken,
		type NotifyToken,
	} from '@/actions/tmgr/notifyTokens';
	import PageContainer from '@/components/layouts/PageContainer.vue';
	import PageHeader from '@/components/layouts/PageHeader.vue';
	import SettingsSection from '@/components/layouts/SettingsSection.vue';
	import { Button } from '@/components/ui/button';
	import { Input } from '@/components/ui/input';
	import { toast } from '@/components/ui/toast';
	import { setDocumentTitle } from '@/composable/useDocumentTitle';
	import { defineComponent, onMounted, ref } from 'vue';

	export default defineComponent({
		name: 'AgentNotificationsSettings',
		components: { PageContainer, PageHeader, SettingsSection, Button, Input },
		setup() {
			setDocumentTitle('Agent notifications');

			const tokens = ref<NotifyToken[]>([]);
			const loading = ref(true);
			const loadError = ref<string | null>(null);
			const label = ref('');
			const creating = ref(false);
			const issuedToken = ref<IssuedNotifyToken | null>(null);
			const copied = ref(false);
			const apiUrl = String(import.meta.env.VITE_API_BASE_URL || '').replace(/\/api\/?$/, '');

			const load = async () => {
				loading.value = true;
				loadError.value = null;
				try {
					tokens.value = await listNotifyTokens();
				} catch {
					loadError.value = 'Could not load tokens.';
				} finally {
					loading.value = false;
				}
			};

			const submitCreate = async () => {
				creating.value = true;
				try {
					issuedToken.value = await createNotifyToken(
						label.value.trim() || undefined,
					);
					label.value = '';
					copied.value = false;
					await load();
				} catch {
					toast({ title: 'Could not create token', variant: 'destructive' });
				} finally {
					creating.value = false;
				}
			};

			const copyToken = async () => {
				if (!issuedToken.value) return;
				try {
					await navigator.clipboard.writeText(issuedToken.value.token);
					copied.value = true;
					setTimeout(() => (copied.value = false), 2000);
				} catch {
					toast({ title: 'Could not copy to clipboard', variant: 'destructive' });
				}
			};

			const revoke = async (token: NotifyToken) => {
				if (
					!window.confirm(
						`Revoke the "${token.label || 'Untitled token'}" token?`,
					)
				)
					return;
				try {
					await revokeNotifyToken(token.id);
					await load();
				} catch {
					toast({ title: 'Could not revoke token', variant: 'destructive' });
				}
			};

			const formatDate = (date: string) => new Date(date).toLocaleDateString();

			onMounted(load);

			return {
				tokens,
				loading,
				loadError,
				label,
				creating,
				issuedToken,
				copied,
				submitCreate,
				copyToken,
				revoke,
				formatDate,
				apiUrl,
			};
		},
	});
</script>
