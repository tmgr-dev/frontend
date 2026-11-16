<template>
	<PageContainer width="narrow">
		<PageHeader :title="$t('settings.slack.title')">
			<template #subtitle>{{ $t('settings.slack.subtitle') }}</template>
		</PageHeader>

		<p
			v-if="notice"
			class="mb-4 text-sm"
			:class="notice.ok ? 'text-status-done' : 'text-destructive'"
			data-testid="slack-notice"
		>
			{{ $t(notice.key) }}
		</p>

		<p v-if="loading" class="text-sm text-ink-subtle">
			{{ $t('settings.slack.loading') }}
		</p>

		<div v-else class="flex flex-col gap-6">
			<SettingsSection
				:title="$t('settings.slack.connection.title')"
				data-testid="slack-connection"
			>
				<template v-if="installation.installed">
					<p class="text-sm text-ink">
						{{
							$t('settings.slack.connection.connectedTo', {
								team: installation.team_name,
							})
						}}
					</p>
					<div class="mt-4 flex flex-wrap gap-2">
						<Button v-if="!installation.linked" :disabled="busy" @click="link">
							{{ $t('settings.slack.link') }}
						</Button>
						<Button v-else variant="outline" :disabled="busy" @click="unlinkMe">
							{{ $t('settings.slack.unlink') }}
						</Button>
						<AlertDialog v-if="canManage">
							<AlertDialogTrigger as-child>
								<Button
									variant="outline"
									class="text-destructive hover:text-destructive"
								>
									{{ $t('settings.slack.disconnect.label') }}
								</Button>
							</AlertDialogTrigger>
							<AlertDialogContent>
								<AlertDialogHeader>
									<AlertDialogTitle>{{
										$t('settings.slack.disconnect.confirmTitle')
									}}</AlertDialogTitle>
									<AlertDialogDescription>
										{{ $t('settings.slack.disconnect.confirmDescription') }}
									</AlertDialogDescription>
								</AlertDialogHeader>
								<AlertDialogFooter>
									<AlertDialogCancel>{{
										$t('common.actions.cancel')
									}}</AlertDialogCancel>
									<AlertDialogAction
										class="bg-destructive text-destructive-foreground shadow-xs hover:bg-destructive/90"
										@click="disconnect"
									>
										{{ $t('settings.slack.disconnect.label') }}
									</AlertDialogAction>
								</AlertDialogFooter>
							</AlertDialogContent>
						</AlertDialog>
					</div>
				</template>
				<template v-else>
					<p class="text-sm text-ink-subtle">
						{{ $t('settings.slack.connection.notConnected') }}
					</p>
					<Button
						v-if="canManage"
						class="mt-4"
						:disabled="busy"
						@click="connect"
					>
						{{ $t('settings.slack.connect') }}
					</Button>
					<p v-else class="mt-4 text-sm text-ink-subtle">
						{{ $t('settings.slack.askAdmin') }}
					</p>
				</template>
			</SettingsSection>

			<template v-if="canManage && installation.installed">
				<SlackLinkedPeople :workspace-id="workspaceId" @changed="reload" />
				<SlackChannelsPanel :workspace-id="Number(workspaceId)" />
			</template>
		</div>
	</PageContainer>
</template>

<script lang="ts">
	import { getWorkspaceModules } from '@/actions/tmgr/modules';
	import {
		confirmSlackInstall,
		confirmSlackLink,
		disconnectSlack,
		getSlackInstallation,
		getSlackInstallUrl,
		getSlackLinkUrl,
		unlinkSlackLink,
		type SlackInstallation,
	} from '@/actions/tmgr/slack';
	import PageContainer from '@/components/layouts/PageContainer.vue';
	import PageHeader from '@/components/layouts/PageHeader.vue';
	import SettingsSection from '@/components/layouts/SettingsSection.vue';
	import SlackChannelsPanel from '@/components/settings/SlackChannelsPanel.vue';
	import SlackLinkedPeople from '@/components/settings/SlackLinkedPeople.vue';
	import {
		AlertDialog,
		AlertDialogAction,
		AlertDialogCancel,
		AlertDialogContent,
		AlertDialogDescription,
		AlertDialogFooter,
		AlertDialogHeader,
		AlertDialogTitle,
		AlertDialogTrigger,
	} from '@/components/ui/alert-dialog';
	import { Button } from '@/components/ui/button';
	import { setDocumentTitle } from '@/composable/useDocumentTitle';
	import { t } from '@/i18n';
	import {
		parseSlackRedirect,
		slackConfirmFailureKey,
		slackReasonKey,
	} from '@/utils/slackRedirect';
	import { computed, defineComponent, onMounted, ref } from 'vue';
	import { useRoute, useRouter } from 'vue-router';
	import { useStore } from 'vuex';

	export default defineComponent({
		name: 'SlackSettingsPage',
		components: {
			PageContainer,
			PageHeader,
			SettingsSection,
			SlackChannelsPanel,
			SlackLinkedPeople,
			AlertDialog,
			AlertDialogAction,
			AlertDialogCancel,
			AlertDialogContent,
			AlertDialogDescription,
			AlertDialogFooter,
			AlertDialogHeader,
			AlertDialogTitle,
			AlertDialogTrigger,
			Button,
		},
		setup() {
			const store = useStore();
			const route = useRoute();
			const router = useRouter();

			const workspaceId = computed(
				() =>
					store.state.user?.settings?.find(
						(s: any) => s.key === 'current_workspace',
					)?.value as string | number,
			);

			const loading = ref(true);
			const busy = ref(false);
			const canManage = ref(false);
			const installation = ref<SlackInstallation>({
				installed: false,
				linked: false,
			});
			const notice = ref<{ ok: boolean; key: string } | null>(null);

			const reload = async () => {
				try {
					installation.value = await getSlackInstallation(workspaceId.value);
				} catch {
					installation.value = { installed: false, linked: false };
				}
			};

			const handleRedirect = async () => {
				const redirect = parseSlackRedirect(route.query);
				if (!redirect) return;
				if (redirect.kind === 'error') {
					notice.value = { ok: false, key: slackReasonKey(redirect.reason) };
				} else {
					try {
						if (redirect.flow === 'install') {
							await confirmSlackInstall(redirect.pending);
						} else {
							await confirmSlackLink(redirect.pending);
						}
						notice.value = {
							ok: true,
							key: `settings.slack.done.${redirect.flow}`,
						};
					} catch (error) {
						notice.value = { ok: false, key: slackConfirmFailureKey(error) };
					}
				}
				await router.replace({ query: {} });
			};

			const redirectTo = async (getUrl: () => Promise<string>) => {
				busy.value = true;
				try {
					window.location.href = await getUrl();
				} catch {
					notice.value = {
						ok: false,
						key: 'settings.slack.errors.slack_unavailable',
					};
					busy.value = false;
				}
			};

			const connect = () =>
				redirectTo(() => getSlackInstallUrl(workspaceId.value));
			const link = () => redirectTo(() => getSlackLinkUrl(workspaceId.value));

			const unlinkMe = async () => {
				busy.value = true;
				try {
					await unlinkSlackLink(workspaceId.value);
					await reload();
				} catch {
					notice.value = { ok: false, key: 'settings.slack.errors.unknown' };
				} finally {
					busy.value = false;
				}
			};

			const disconnect = async () => {
				try {
					await disconnectSlack(workspaceId.value);
					await reload();
				} catch {
					notice.value = { ok: false, key: 'settings.slack.errors.unknown' };
				}
			};

			onMounted(async () => {
				setDocumentTitle(t('settings.slack.title'));
				await handleRedirect();
				const modules = await getWorkspaceModules(workspaceId.value).catch(
					() => null,
				);
				canManage.value = !!modules?.canManage;
				await reload();
				loading.value = false;
			});

			return {
				workspaceId,
				loading,
				busy,
				canManage,
				installation,
				notice,
				reload,
				connect,
				link,
				unlinkMe,
				disconnect,
			};
		},
	});
</script>
