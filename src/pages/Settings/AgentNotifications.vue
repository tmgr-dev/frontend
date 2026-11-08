<template>
	<PageContainer width="narrow">
		<PageHeader :title="$t('settingsPages.agentNotifications.title')">
			<template #subtitle>
				{{ $t('settingsPages.agentNotifications.subtitle') }}
			</template>
		</PageHeader>

		<AiAgentsOverview class="mb-6" />

		<SettingsSection
			:title="$t('settingsPages.agentNotifications.newToken')"
			class="mb-6"
			data-tour="agent-notify.connect"
		>
			<form class="flex flex-col gap-3" @submit.prevent="submitCreate">
				<label class="flex flex-col gap-1">
					<span class="text-sm font-medium">{{
						$t('settingsPages.agentNotifications.labelOptional')
					}}</span>
					<Input
						:model-value="label"
						maxlength="60"
						:placeholder="
							$t('settingsPages.agentNotifications.labelPlaceholder')
						"
						@update:model-value="(v) => (label = String(v))"
					/>
				</label>
				<div>
					<Button type="submit" :disabled="creating">
						{{
							creating
								? $t('settingsPages.agentNotifications.creating')
								: $t('settingsPages.agentNotifications.createToken')
						}}
					</Button>
				</div>
			</form>

			<div
				v-if="issuedToken"
				class="mt-4 flex flex-col gap-2 border-t border-border pt-4"
			>
				<p class="text-sm font-medium text-amber-600 dark:text-amber-400">
					{{ $t('settingsPages.agentNotifications.copyNow') }}
				</p>
				<div class="flex items-center gap-2">
					<code
						class="min-w-0 flex-1 overflow-x-auto rounded bg-muted px-2 py-1.5 font-mono text-xs"
					>
						{{ issuedToken.token }}
					</code>
					<Button type="button" variant="outline" size="sm" @click="copyToken">
						{{
							copied ? $t('common.actions.copied') : $t('common.actions.copy')
						}}
					</Button>
				</div>
				<div class="flex items-center gap-2">
					<Button type="button" size="sm" @click="dialogOpen = true">
						{{ $t('settingsPages.agentNotifications.showSetup') }}
					</Button>
					<Button
						type="button"
						variant="outline"
						size="sm"
						@click="dismissToken"
					>
						{{ $t('common.actions.done') }}
					</Button>
				</div>
			</div>
		</SettingsSection>

		<p v-if="loadError" class="mb-4 text-sm text-destructive">
			{{ loadError }}
		</p>

		<SettingsSection :title="$t('settingsPages.agentNotifications.tokens')">
			<p v-if="loading" class="text-sm text-muted-foreground">
				{{ $t('settingsPages.agentNotifications.loadingTokens') }}
			</p>
			<div v-else class="flex flex-col gap-2">
				<p v-if="tokens.length === 0" class="text-sm text-muted-foreground">
					{{ $t('settingsPages.agentNotifications.noTokens') }}
				</p>
				<div
					v-for="token in tokens"
					:key="token.id"
					class="flex items-center justify-between gap-3 rounded border border-border p-2"
				>
					<div class="min-w-0">
						<p class="truncate text-sm font-medium">
							{{
								token.label || $t('settingsPages.agentNotifications.untitled')
							}}
						</p>
						<p class="truncate text-xs text-muted-foreground">
							{{
								$t('settingsPages.agentNotifications.meta', {
									prefix: token.prefix,
									created: formatDate(token.created_at),
									lastUsed: token.last_used_at
										? formatDate(token.last_used_at)
										: $t('settingsPages.agentNotifications.never'),
								})
							}}
						</p>
					</div>
					<Button
						variant="outline"
						size="sm"
						class="shrink-0"
						@click="revoke(token)"
					>
						{{ $t('settingsPages.agentNotifications.revoke') }}
					</Button>
				</div>
			</div>
		</SettingsSection>

		<SettingsSection
			:title="$t('settingsPages.agentNotifications.howToConnect')"
			class="mt-6"
		>
			<div class="flex flex-col gap-3 text-sm text-muted-foreground">
				<p>
					<i18n-t
						keypath="settingsPages.agentNotifications.haveToken"
						tag="span"
					>
						<template #placeholder>
							<code class="rounded bg-muted px-1 py-0.5 font-mono text-xs">{{
								$t('settingsPages.agentNotifications.yourToken')
							}}</code>
						</template>
					</i18n-t>
				</p>
				<Collapsible v-model:open="snippetsOpen" class="flex flex-col gap-3">
					<CollapsibleTrigger as-child>
						<Button type="button" variant="outline" size="sm" class="w-fit">
							{{
								snippetsOpen
									? $t('settingsPages.agentNotifications.hideSnippets')
									: $t('settingsPages.agentNotifications.showSnippets')
							}}
						</Button>
					</CollapsibleTrigger>
					<CollapsibleContent>
						<AgentConnectTabs />
					</CollapsibleContent>
				</Collapsible>
				<p>
					<router-link to="/docs/agents" class="text-primary hover:underline">
						{{ $t('settingsPages.agentNotifications.fullGuide') }}
					</router-link>
				</p>
			</div>
		</SettingsSection>

		<AlarmPhoneSection id="alarm-phone" class="mt-6" />

		<PhonesSection id="phones" class="mt-6" />

		<Dialog
			:open="dialogOpen && !!issuedToken"
			@update:open="(v) => (dialogOpen = v)"
		>
			<DialogContent
				class="max-h-[90vh] overflow-y-auto sm:max-w-[680px] [&>*]:min-w-0"
			>
				<DialogHeader>
					<DialogTitle>{{
						$t('settingsPages.agentNotifications.connectTitle')
					}}</DialogTitle>
				</DialogHeader>
				<template v-if="issuedToken">
					<div class="flex min-w-0 flex-col gap-2">
						<p class="text-sm font-medium text-amber-600 dark:text-amber-400">
							{{ $t('settingsPages.agentNotifications.copyNow') }}
						</p>
						<CodeSnippet
							:code="issuedToken.token"
							:label="$t('settingsPages.agentNotifications.notifyTokenLabel')"
						/>
					</div>

					<AgentConnectTabs
						:token="issuedToken.token"
						data-tour="agent-notify.command"
					/>

					<div class="flex flex-col gap-2 border-t border-border pt-4">
						<div class="flex flex-wrap items-center gap-3">
							<Button
								type="button"
								data-tour="agent-notify.test"
								variant="outline"
								size="sm"
								:disabled="testing"
								@click="sendTest"
							>
								{{
									testing
										? $t('settingsPages.agentNotifications.sending')
										: $t('settingsPages.agentNotifications.sendTest')
								}}
							</Button>
							<p v-if="testMessage" class="text-sm" role="status">
								{{ testMessage }}
							</p>
						</div>
						<p class="text-sm text-muted-foreground">
							<i18n-t
								keypath="settingsPages.agentNotifications.phoneCall"
								tag="span"
							>
								<template #link>
									<button
										type="button"
										class="text-primary hover:underline"
										@click="goToAlarmPhone"
									>
										{{ $t('settingsPages.agentNotifications.setupAlarm') }}
									</button>
								</template>
							</i18n-t>
						</p>
					</div>
				</template>
				<DialogFooter>
					<Button type="button" @click="dismissToken">{{
						$t('common.actions.done')
					}}</Button>
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
		type NotifyToken,
		type TestNotificationResult,
	} from '@/actions/tmgr/notifyTokens';
	import AgentConnectTabs from '@/components/agents/AgentConnectTabs.vue';
	import AiAgentsOverview from '@/components/agents/AiAgentsOverview.vue';
	import CodeSnippet from '@/components/agents/CodeSnippet.vue';
	import PageContainer from '@/components/layouts/PageContainer.vue';
	import PageHeader from '@/components/layouts/PageHeader.vue';
	import SettingsSection from '@/components/layouts/SettingsSection.vue';
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
	import { currentLocale, t } from '@/i18n';
	import AlarmPhoneSection from '@/pages/Settings/AlarmPhoneSection.vue';
	import PhonesSection from '@/pages/Settings/PhonesSection.vue';
	import { computed, defineComponent, nextTick, onMounted, ref } from 'vue';

	export default defineComponent({
		name: 'AgentNotificationsSettings',
		components: {
			PageContainer,
			PageHeader,
			AiAgentsOverview,
			SettingsSection,
			AlarmPhoneSection,
			PhonesSection,
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
			setDocumentTitle(t('settingsPages.agentNotifications.title'));

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
					loadError.value = t('settingsPages.agentNotifications.loadFailed');
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
					toast({
						title: t('settingsPages.agentNotifications.createFailed'),
						variant: 'destructive',
					});
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
						return t('settingsPages.agentNotifications.test.sent', {
							channels: result.channels.join(', '),
						});
					case 'no_channels':
						return t('settingsPages.agentNotifications.test.noChannels');
					case 'deduplicated':
						return t('settingsPages.agentNotifications.test.deduplicated');
					case 'rate_limited':
						return result.retryAfter
							? t('settingsPages.agentNotifications.test.rateLimitedIn', {
									seconds: result.retryAfter,
								})
							: t('settingsPages.agentNotifications.test.rateLimited');
					case 'unauthorized':
						return t('settingsPages.agentNotifications.test.unauthorized');
					default:
						return t('settingsPages.agentNotifications.test.unreachable');
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
					toast({
						title: t('settingsPages.agentNotifications.copyFailed'),
						variant: 'destructive',
					});
				}
			};

			const revoke = async (token: NotifyToken) => {
				if (
					!window.confirm(
						t('settingsPages.agentNotifications.confirmRevoke', {
							label:
								token.label || t('settingsPages.agentNotifications.untitled'),
						}),
					)
				)
					return;
				try {
					await revokeNotifyToken(token.id);
					await load();
				} catch {
					toast({
						title: t('settingsPages.agentNotifications.revokeFailed'),
						variant: 'destructive',
					});
				}
			};

			const formatDate = (date: string) =>
				new Date(date).toLocaleDateString(currentLocale());

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
