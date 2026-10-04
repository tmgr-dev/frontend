<template>
	<PageContainer width="narrow">
		<PageHeader title="Agent notifications">
			<template #subtitle>
				Get a push notification on your phone when Claude Code, Codex or
				another AI agent needs you — the recipient is always you, the token
				owner, and a token works across every workspace you're in. Create a
				token, then connect Claude Code, Codex or Cursor in one step.
			</template>
		</PageHeader>

		<AiAgentsOverview class="mb-6" />

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
				<div class="flex items-center gap-2">
					<Button type="button" size="sm" @click="dialogOpen = true">
						Show setup
					</Button>
					<Button type="button" variant="outline" size="sm" @click="dismissToken">
						Done
					</Button>
				</div>
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

		<SettingsSection title="How to connect" class="mt-6">
			<div class="flex flex-col gap-3 text-sm text-muted-foreground">
				<p>
					Already have a token? Use these snippets and replace
					<code class="rounded bg-muted px-1 py-0.5 font-mono text-xs">&lt;your token&gt;</code>
					with it.
				</p>
				<Collapsible v-model:open="snippetsOpen" class="flex flex-col gap-3">
					<CollapsibleTrigger as-child>
						<Button type="button" variant="outline" size="sm" class="w-fit">
							{{ snippetsOpen ? 'Hide snippets' : 'Show snippets' }}
						</Button>
					</CollapsibleTrigger>
					<CollapsibleContent>
						<AgentConnectTabs />
					</CollapsibleContent>
				</Collapsible>
				<p>
					<router-link to="/docs/agents" class="text-primary hover:underline">
						Full setup guide
					</router-link>
				</p>
			</div>
		</SettingsSection>

		<AlarmPhoneSection id="alarm-phone" class="mt-6" />

		<Dialog :open="dialogOpen && !!issuedToken" @update:open="(v) => (dialogOpen = v)">
			<DialogContent class="max-h-[90vh] overflow-y-auto sm:max-w-[680px] [&>*]:min-w-0">
				<DialogHeader>
					<DialogTitle>Connect your agent</DialogTitle>
				</DialogHeader>
				<template v-if="issuedToken">
					<div class="flex min-w-0 flex-col gap-2">
						<p class="text-sm font-medium text-amber-600 dark:text-amber-400">
							Copy this token now — you won't see it again.
						</p>
						<CodeSnippet :code="issuedToken.token" label="Notify token" />
					</div>

					<AgentConnectTabs :token="issuedToken.token" />

					<div class="flex flex-col gap-2 border-t border-border pt-4">
						<div class="flex flex-wrap items-center gap-3">
							<Button
								type="button"
								variant="outline"
								size="sm"
								:disabled="testing"
								@click="sendTest"
							>
								{{ testing ? 'Sending…' : 'Send test notification' }}
							</Button>
							<p v-if="testMessage" class="text-sm" role="status">
								{{ testMessage }}
							</p>
						</div>
						<p class="text-sm text-muted-foreground">
							Want a phone call for incidents?
							<button
								type="button"
								class="text-primary hover:underline"
								@click="goToAlarmPhone"
							>
								Set up an alarm phone
							</button>
							below.
						</p>
					</div>
				</template>
				<DialogFooter>
					<Button type="button" @click="dismissToken">Done</Button>
				</DialogFooter>
			</DialogContent>
		</Dialog>
	</PageContainer>
</template>

<script lang="ts">
	import {
		createNotifyToken,
		listNotifyTokens,
		revokeNotifyToken,
		sendTestNotification,
		type IssuedNotifyToken,
		type TestNotificationResult,
		type NotifyToken,
	} from '@/actions/tmgr/notifyTokens';
	import PageContainer from '@/components/layouts/PageContainer.vue';
	import PageHeader from '@/components/layouts/PageHeader.vue';
	import AiAgentsOverview from '@/components/agents/AiAgentsOverview.vue';
	import SettingsSection from '@/components/layouts/SettingsSection.vue';
	import AgentConnectTabs from '@/components/agents/AgentConnectTabs.vue';
	import CodeSnippet from '@/components/agents/CodeSnippet.vue';
	import AlarmPhoneSection from '@/pages/Settings/AlarmPhoneSection.vue';
	import { Button } from '@/components/ui/button';
	import {
		Collapsible,
		CollapsibleContent,
		CollapsibleTrigger,
	} from '@/components/ui/collapsible';
	import {
		Dialog,
		DialogContent,
		DialogFooter,
		DialogHeader,
		DialogTitle,
	} from '@/components/ui/dialog';
	import { Input } from '@/components/ui/input';
	import { toast } from '@/components/ui/toast';
	import { setDocumentTitle } from '@/composable/useDocumentTitle';
	import { computed, defineComponent, nextTick, onMounted, ref } from 'vue';

	export default defineComponent({
		name: 'AgentNotificationsSettings',
		components: {
			PageContainer,
			PageHeader,
			AiAgentsOverview,
			SettingsSection,
			AlarmPhoneSection,
			AgentConnectTabs,
			CodeSnippet,
			Button,
			Collapsible,
			CollapsibleContent,
			CollapsibleTrigger,
			Dialog,
			DialogContent,
			DialogFooter,
			DialogHeader,
			DialogTitle,
			Input,
		},
		setup() {
			setDocumentTitle('Agent notifications');

			const tokens = ref<NotifyToken[]>([]);
			const loading = ref(true);
			const loadError = ref<string | null>(null);
			const label = ref('');
			const creating = ref(false);
			const issuedToken = ref<IssuedNotifyToken | null>(null);
			const copied = ref(false);
			const dialogOpen = ref(false);
			const snippetsOpen = ref(false);
			const testing = ref(false);
			const testResult = ref<TestNotificationResult | null>(null);

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
					testResult.value = null;
					testing.value = false;
					dialogOpen.value = true;
					await load();
				} catch {
					toast({ title: 'Could not create token', variant: 'destructive' });
				} finally {
					creating.value = false;
				}
			};

			const dismissToken = () => {
				dialogOpen.value = false;
				issuedToken.value = null;
				testResult.value = null;
				testing.value = false;
			};

			const sendTest = async () => {
				if (!issuedToken.value || testing.value) return;
				const token = issuedToken.value.token;
				testing.value = true;
				testResult.value = null;
				const result = await sendTestNotification(
					token,
					import.meta.env.VITE_API_BASE_URL,
				);
				if (issuedToken.value?.token !== token) return;
				testResult.value = result;
				testing.value = false;
			};

			const testMessage = computed(() => {
				const result = testResult.value;
				if (!result) return '';
				switch (result.kind) {
					case 'sent':
						return `Sent — check your phone. (via ${result.channels.join(', ')})`;
					case 'no_channels':
						return 'Nothing was delivered: install the TMGR mobile app and allow notifications, or link Telegram in Profile.';
					case 'deduplicated':
						return 'Already sent a moment ago.';
					case 'rate_limited':
						return result.retryAfter
							? `Too many notifications — try again in ${result.retryAfter} s.`
							: 'Too many notifications — try again shortly.';
					case 'unauthorized':
						return 'This token was rejected. Create a new one.';
					default:
						return 'Could not reach TMGR.';
				}
			});

			const goToAlarmPhone = async () => {
				dialogOpen.value = false;
				await nextTick();
				document
					.getElementById('alarm-phone')
					?.scrollIntoView({ behavior: 'smooth', block: 'start' });
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
				dialogOpen,
				snippetsOpen,
				testing,
				testMessage,
				dismissToken,
				sendTest,
				goToAlarmPhone,
			};
		},
	});
</script>
