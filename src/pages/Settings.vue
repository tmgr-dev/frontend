<template>
	<div>
		<BaseLayout width="narrow" :title="pageTitle">
			<template #body>
				<AsyncContent
					:pending="initialPending"
					:loaded="initialLoaded"
					:error="initialError"
					:retry="loadInitialSettings"
					:label="$t('settings.loading')"
				>
					<div class="flex flex-col gap-6">
						<div
							v-if="isNotification && !localOnly"
							class="flex flex-col gap-6"
						>
							<NotificationSettingsForm />

							<SettingsSection :title="$t('settings.push.title')">
								<div class="flex flex-col gap-4">
									<div class="flex flex-wrap gap-2">
										<Button
											v-if="!pusherBeamsUserId"
											variant="outline"
											size="sm"
											@click="togglePushes"
										>
											{{ $t('settings.push.enable') }}
										</Button>
										<Button
											variant="outline"
											size="sm"
											@click="testWebPushNotifications"
										>
											{{ $t('settings.push.test') }}
										</Button>
									</div>
									<SettingsRow
										v-slot="{ labelId, descriptionId }"
										:label="$t('settings.push.showTooltips')"
									>
										<Switch
											:checked="userSettings.showTooltips"
											:aria-labelledby="labelId"
											:aria-describedby="descriptionId"
											@update:checked="
												(value) => (userSettings.showTooltips = value)
											"
										/>
									</SettingsRow>
								</div>
								<template #footer>
									<Button
										:disabled="savingSettings"
										:aria-busy="savingSettings"
										@click="updateSettings"
									>
										{{ $t('common.actions.save') }}
									</Button>
								</template>
							</SettingsSection>
						</div>

						<div
							v-if="isDesktopSection && isDesktop"
							class="flex flex-col gap-6"
						>
							<SettingsSection :title="$t('settings.desktop.gitTracking')">
								<DesktopGitTrackSettings />
							</SettingsSection>
							<SettingsSection :title="$t('settings.desktop.shortcuts')">
								<DesktopShortcutsSettings />
							</SettingsSection>
						</div>

						<RunnerExperimentalSettings v-if="isExperimental" />

						<div v-if="isAbout" class="flex flex-col gap-6">
							<SettingsSection title="TMGR">
								<div class="flex flex-col gap-2 text-sm">
									<p v-if="appVersion" class="text-muted-foreground">
										{{ $t('settings.about.version', { version: appVersion }) }}
									</p>
									<a
										href="https://tmgr.dev"
										target="_blank"
										rel="noopener noreferrer"
										class="text-primary hover:underline"
									>
										tmgr.dev
									</a>
									<a
										href="https://tmgr.dev/cli"
										target="_blank"
										rel="noopener noreferrer"
										class="text-primary hover:underline"
										data-testid="settings-cli-link"
									>
										{{ $t('settings.about.cliLink') }}
									</a>
								</div>
							</SettingsSection>
							<TipsSettings />
							<PromotionsList
								v-if="promotionsEnabled && !localOnly"
								placement="about"
								:section-title="$t('settings.about.moreTitle')"
								:section-description="$t('settings.about.moreDescription')"
							/>
						</div>

						<PrivacySettings v-if="isPrivacy" />

						<AgentUsageSync v-if="isAgentWork" />

						<SettingsSection
							v-if="isTheme"
							data-tour="settings.theme"
							:title="$t('settings.theme.title')"
							:description="$t('settings.theme.description')"
						>
							<ThemePicker />
						</SettingsSection>

						<LanguageSetting v-if="isTheme" />

						<TimeZoneSetting v-if="isTheme && !localOnly" />

						<div v-if="isProfile && localOnly" class="flex flex-col gap-6">
							<LocalProfileSettings />
							<LocalAccountSection />
						</div>

						<div v-if="isLocalData" class="flex flex-col gap-6">
							<LocalDataSettings />
						</div>

						<div v-if="isProfile && !localOnly" class="flex flex-col gap-6">
							<profile
								:standalone="false"
								@password-changed="onPasswordChanged"
							/>

							<SettingsSection
								v-if="telegramEnabled"
								data-tour="settings.telegram"
								:title="$t('settings.telegram.title')"
							>
								<div
									v-if="user.telegram_username"
									class="flex flex-wrap items-center gap-3"
								>
									<span class="text-sm text-status-done">
										{{
											$t('settings.telegram.connectedAs', {
												username: user.telegram_username,
											})
										}}
									</span>
									<Button
										variant="outline"
										size="sm"
										class="text-destructive"
										@click="unlinkTelegram"
									>
										{{ $t('settings.telegram.unlink') }}
									</Button>
								</div>
								<Button v-else @click="generateTelegramLink">
									{{ $t('settings.telegram.connect') }}
								</Button>
							</SettingsSection>
						</div>

						<div v-if="isDevice && !localOnly" class="flex flex-col gap-6">
							<SettingsSection
								v-if="mcpEnabled"
								data-tour="settings.device-token"
								:title="$t('settings.device.tokenTitle')"
							>
								<div class="flex max-w-xl flex-col gap-1.5">
									<Label for="smart-device-token">{{
										$t('settings.device.tokenTitle')
									}}</Label>
									<div class="flex flex-wrap items-center gap-2">
										<Input
											v-if="tokenState === 'fresh'"
											id="smart-device-token"
											:type="showToken ? 'text' : 'password'"
											:model-value="freshToken"
											class="min-w-[220px] flex-1"
											readonly
										/>
										<Input
											v-else
											id="smart-device-token"
											class="min-w-[220px] flex-1"
											:placeholder="
												tokenState === 'hidden'
													? $t('settings.device.tokenHidden')
													: $t('settings.device.tokenNeeded')
											"
											readonly
											:disabled="tokenState === 'hidden'"
										/>
										<Button
											v-if="tokenState === 'fresh'"
											variant="outline"
											size="sm"
											@click="copyToken"
										>
											<Check v-if="tokenCopied" class="text-status-done" />
											<Copy v-else />
											{{
												tokenCopied
													? $t('common.actions.copied')
													: $t('common.actions.copy')
											}}
										</Button>
										<Button
											v-if="tokenState === 'fresh'"
											variant="outline"
											size="sm"
											@click="showToken = !showToken"
										>
											<EyeOff v-if="showToken" />
											<Eye v-else />
											{{
												showToken
													? $t('settings.device.hide')
													: $t('settings.device.show')
											}}
										</Button>
									</div>
									<p
										v-if="tokenState === 'fresh'"
										class="text-xs text-ink-subtle"
									>
										{{ $t('settings.device.copyNow') }}
									</p>
									<p v-if="hasSmartDeviceToken" class="text-xs text-ink-subtle">
										{{
											$t('settings.device.created', {
												date: formatDate(user.smart_device_token_created_at),
											})
										}}
									</p>
								</div>
								<template #footer>
									<Button @click="generateSmartDeviceToken">
										{{
											hasSmartDeviceToken
												? $t('settings.device.generateNew')
												: $t('settings.device.generate')
										}}
									</Button>
									<Button
										v-if="hasSmartDeviceToken"
										variant="destructive"
										@click="revokeSmartDeviceToken"
									>
										{{ $t('settings.device.revoke') }}
									</Button>
								</template>
							</SettingsSection>

							<SettingsSection
								v-if="mcpEnabled"
								:title="$t('settings.device.howTo.title')"
							>
								<div class="flex flex-col gap-2 text-sm text-ink-subtle">
									<p>{{ $t('settings.device.howTo.step1') }}</p>
									<p>{{ $t('settings.device.howTo.step2') }}</p>
									<code
										class="block overflow-x-auto rounded-md border border-border bg-muted px-3 py-2 font-mono text-xs text-ink"
									>
										X-Smart-Device-Token: your_token_here
									</code>
									<p>{{ $t('settings.device.howTo.secure') }}</p>
								</div>
							</SettingsSection>

							<SettingsSection
								v-if="mcpEnabled"
								:title="$t('settings.device.mcp.title')"
							>
								<div class="flex flex-col gap-3 text-sm text-ink-subtle">
									<i18n-t keypath="settings.device.mcp.intro" tag="p">
										<template #command>
											<CommandChip :command="mcpAddCommand" />
										</template>
									</i18n-t>
									<div>
										<p class="mb-1 font-medium text-ink">
											{{ $t('settings.device.mcp.serverUrl') }}
										</p>
										<code
											class="block overflow-x-auto rounded-md border border-border bg-muted px-3 py-2 font-mono text-xs text-ink"
										>
											{{ mcpUrl }}
										</code>
									</div>
									<div>
										<p class="mb-1 font-medium text-ink">
											{{ $t('settings.device.mcp.clientConfig') }}
										</p>
										<pre
											class="overflow-x-auto rounded-md border border-border bg-muted px-3 py-2 font-mono text-xs whitespace-pre text-ink"
											>{{ mcpClientConfig }}</pre>
									</div>
									<p>{{ $t('settings.device.mcp.oauth') }}</p>
									<i18n-t keypath="settings.device.mcp.notify" tag="p">
										<template #link>
											<router-link
												to="/settings/agent-notifications"
												class="text-primary hover:underline"
												>{{ $t('settings.device.mcp.notifyLink') }}</router-link
											>
										</template>
									</i18n-t>
								</div>
							</SettingsSection>

							<SettingsSection :title="$t('settings.device.docs.title')">
								<ul class="flex flex-col gap-1 text-sm">
									<li>
										<a
											:href="`${docsBaseUrl}/docs/smart-devices.html`"
											target="_blank"
											rel="noopener"
											class="text-primary hover:underline"
										>
											{{ $t('settings.device.docs.api') }}
										</a>
									</li>
									<li>
										<a
											:href="`${docsBaseUrl}/docs/mcp.html`"
											target="_blank"
											rel="noopener"
											class="text-primary hover:underline"
										>
											{{ $t('settings.device.docs.mcp') }}
										</a>
									</li>
								</ul>
							</SettingsSection>
						</div>
					</div>
				</AsyncContent>
				<!-- Confirmation Modal -->
				<Transition name="fade">
					<confirm
						v-if="confirm"
						:body="confirm.body"
						:title="confirm.title"
						@onCancel="confirm = undefined"
						@onOk="confirm.action()"
					>
						<template #body-content>
							<p>{{ confirm.body }}</p>

							<a
								v-if="confirm.link"
								class="mt-2 block text-primary hover:underline"
								:href="confirm.link"
								target="_blank"
							>
								{{ $t('settings.telegram.open') }}
							</a>
						</template>
					</confirm>
				</Transition>
			</template>
		</BaseLayout>
	</div>
</template>

<script>
	import AsyncContent from '@/components/async/AsyncContent.vue';

	import { withCurrentAgentUsageSync } from '@/actions/tmgr/agentUsage';
	import { sendNotification } from '@/actions/tmgr/notifications';
	import {
		generateSmartDeviceToken,
		revokeSmartDeviceToken,
	} from '@/actions/tmgr/smart-devices';
	import { generateLink, unlink } from '@/actions/tmgr/telegram';
	import {
		getUser,
		getUserSettingsV2,
		updateUserSettings,
		updateUserSettingsV2,
	} from '@/actions/tmgr/user';
	import DesktopGitTrackSettings from '@/components/desktop/DesktopGitTrackSettings.vue';
	import DesktopShortcutsSettings from '@/components/desktop/DesktopShortcutsSettings.vue';
	import LocalAccountSection from '@/components/desktop/LocalAccountSection.vue';
	import LocalDataSettings from '@/components/desktop/LocalDataSettings.vue';
	import LocalProfileSettings from '@/components/desktop/LocalProfileSettings.vue';
	import CommandChip from '@/components/general/CommandChip.vue';
	import Confirm from '@/components/general/Confirm.vue';
	import ThemePicker from '@/components/general/ThemePicker.vue';
	import LanguageSetting from '@/components/general/LanguageSetting.vue';
	import TimeZoneSetting from '@/components/general/TimeZoneSetting.vue';
	import SettingsRow from '@/components/layouts/SettingsRow.vue';
	import SettingsSection from '@/components/layouts/SettingsSection.vue';
	import NotificationSettingsForm from '@/components/notifications/NotificationSettingsForm.vue';
	import PrivacySettings from '@/components/privacy/PrivacySettings.vue';
	import PromotionsList from '@/components/promotions/PromotionsList.vue';
	import RunnerExperimentalSettings from '@/components/runner/RunnerExperimentalSettings.vue';
	import AgentUsageSync from '@/components/settings/AgentUsageSync.vue';
	import {
		BreadcrumbItem,
		BreadcrumbLink,
		BreadcrumbSeparator,
	} from '@/components/ui/breadcrumb';
	import { Button } from '@/components/ui/button';
	import { Input } from '@/components/ui/input';
	import { Label } from '@/components/ui/label';
	import { Switch } from '@/components/ui/switch';
	import { useToast } from '@/components/ui/toast';
	import { setDocumentTitle } from '@/composable/useDocumentTitle';
	import Profile from '@/pages/Profile.vue';
	import { isRunnerSupported } from '@/runner/api';
	import TipsSettings from '@/tours/ui/TipsSettings.vue';
	import { t } from '@/i18n';
	import { isDesktopApp } from '@/utils/desktop';
	import { resolveMcpUrl } from '@/utils/mcpConnectSnippets';
	import { writeNtfySubscribed } from '@/utils/ntfySubscription';
	import { serverMessageFrom } from '@/utils/serverError';
	import {
		hasSmartDeviceToken,
		smartDeviceTokenState,
	} from '@/utils/smartDeviceToken';
	import {
		Check,
		CircleCheckBigIcon,
		Copy,
		Eye,
		EyeOff,
	} from 'lucide-vue-next';

	export default {
		name: 'Settings',
		components: {
			AsyncContent,
			BreadcrumbSeparator,
			BreadcrumbItem,
			BreadcrumbLink,
			Profile,
			Confirm,
			NotificationSettingsForm,
			DesktopGitTrackSettings,
			DesktopShortcutsSettings,
			RunnerExperimentalSettings,
			LocalAccountSection,
			LocalDataSettings,
			LocalProfileSettings,
			ThemePicker,
			LanguageSetting,
			TimeZoneSetting,
			CommandChip,
			SettingsSection,
			PromotionsList,
			SettingsRow,
			PrivacySettings,
			AgentUsageSync,
			TipsSettings,
			Button,
			Input,
			Label,
			Switch,
			Check,
			Copy,
			Eye,
			EyeOff,
		},
		setup() {
			return { toaster: useToast() };
		},
		created() {
			this.handleTabFromQuery();
		},
		data: () => ({
			savingSettings: false,
			initialPending: true,
			initialLoaded: false,
			initialError: null,
			availableSettings: [],
			settings: [],
			user: {},
			confirm: null,
			isNotification: false,
			isProfile: false,
			isDevice: false,
			isTheme: false,
			isDesktopSection: false,
			isExperimental: false,
			isLocalData: false,
			isAbout: false,
			isPrivacy: false,
			isAgentWork: false,
			appVersion: '',
			isDesktop: isDesktopApp(),
			telegramLink: null,
			showToken: false,
			tokenCopied: false,
			freshToken: null,
		}),
		watch: {
			'$route.query': {
				handler: 'handleTabFromQuery',
				immediate: true,
			},
		},
		computed: {
			localOnly() {
				return this.$store.getters.isLocalOnly === true;
			},
			telegramEnabled() {
				return this.$store.getters['featureToggles/isFeatureEnabled'](
					'telegram',
				);
			},
			promotionsEnabled() {
				return this.$store.getters['featureToggles/isFeatureEnabled'](
					'promotions',
				);
			},
			mcpEnabled() {
				return this.$store.getters['featureToggles/isFeatureEnabled']('mcp');
			},
			pageTitle() {
				if (this.isProfile) return t('settings.page.profile');
				if (this.isDevice) return t('settings.page.devices');
				if (this.isNotification) return t('settings.page.notifications');
				if (this.isTheme) return t('settings.page.theme');
				if (this.isExperimental) return t('settings.page.experimental');
				if (this.isDesktopSection) return t('settings.page.desktop');
				if (this.isLocalData) return t('settings.page.localData');
				if (this.isPrivacy) return t('settings.page.privacy');
				if (this.isAgentWork) return t('settings.page.agentWork');
				if (this.isAbout) return t('settings.page.about');
				return t('settings.page.settings');
			},
			hasSmartDeviceToken() {
				return hasSmartDeviceToken(this.user);
			},
			tokenState() {
				return smartDeviceTokenState(this.user, this.freshToken);
			},
			userSettings() {
				return this.$store.state.userSettings || {};
			},
			pusherBeamsUserId() {
				return this.$store.getters.getPusherBeamsUserId;
			},
			mcpUrl() {
				return resolveMcpUrl(
					import.meta.env.VITE_API_BASE_URL,
					window.location.origin,
				);
			},
			mcpAddCommand() {
				return `claude mcp add --transport http tmgr ${this.mcpUrl} --header "X-Smart-Device-Token: <token>"`;
			},
			docsBaseUrl() {
				const apiBaseUrl = import.meta.env.VITE_API_BASE_URL || '/api/';

				try {
					const url = new URL(apiBaseUrl, window.location.origin);
					const basePath = url.pathname
						.replace(/\/api\/?$/, '')
						.replace(/\/$/, '');

					return `${url.origin}${basePath}`;
				} catch (error) {
					return '';
				}
			},
			mcpClientConfig() {
				return JSON.stringify(
					{
						mcpServers: {
							tmgr: {
								type: 'http',
								url: this.mcpUrl,
								headers: {
									'X-Smart-Device-Token': '<paste token from the field above>',
								},
							},
						},
					},
					null,
					2,
				);
			},
		},
		async mounted() {
			setDocumentTitle(t('settings.page.settings'));
			await this.loadInitialSettings();
		},
		methods: {
			async loadInitialSettings() {
				this.initialPending = true;
				this.initialError = null;
				try {
					if (this.localOnly) {
						this.user = await getUser();
					} else {
						const [user, settings] = await Promise.all([
							getUser(),
							getUserSettingsV2(),
						]);
						this.user = user;
						this.initSettings(settings, user.settings);
						this.availableSettings = settings;
					}
					this.initialLoaded = true;
				} catch {
					this.initialError = t('settings.loadFailed');
				} finally {
					this.initialPending = false;
				}
			},
			async copyToken() {
				if (!this.freshToken || !navigator?.clipboard) return;
				try {
					await navigator.clipboard.writeText(this.freshToken);
					this.tokenCopied = true;
					setTimeout(() => {
						this.tokenCopied = false;
					}, 2000);
				} catch (error) {
					console.error('Copy failed', error);
				}
			},
			formatDate(date) {
				if (!date) return '';
				return new Date(date).toLocaleString();
			},

			handleTabFromQuery() {
				if (this.$route.name !== 'Settings') return;
				const tab = this.$route.query.tab;
				if (!tab) {
					this.openWorkspaceSettings();
				} else {
					switch (tab.toLowerCase()) {
						case 'notification':
							if (this.localOnly) this.showProfileSettings();
							else this.showNotificationSettings();
							break;
						case 'workspace':
							this.openWorkspaceSettings();
							break;
						case 'profile':
							this.showProfileSettings();
							break;
						case 'device':
							if (this.localOnly) this.showProfileSettings();
							else this.showDeviceSettings();
							break;
						case 'theme':
							this.showThemeSettings();
							break;
						case 'desktop':
							this.showDesktopSettings();
							break;
						case 'experimental':
							if (isRunnerSupported()) this.showExperimentalSettings();
							break;
						case 'data':
							this.showLocalDataSettings();
							break;
						case 'privacy':
							this.showPrivacySettings();
							break;
						case 'agent-work':
							if (this.localOnly) this.showProfileSettings();
							else this.showAgentWorkSettings();
							break;
						case 'about':
							this.showAboutSettings();
							break;
					}
				}
			},

			showNotificationSettings() {
				this.isLocalData = false;
				this.isPrivacy = false;
				this.isAgentWork = false;
				this.isAbout = false;
				this.isProfile = false;
				this.isNotification = true;
				this.isDevice = false;
				this.isTheme = false;
				this.isDesktopSection = false;
				this.isExperimental = false;
				this.updateQueryParam('notification');
			},

			openWorkspaceSettings() {
				this.$router.replace('/settings/workspaces').catch(() => {});
			},

			showProfileSettings() {
				this.isLocalData = false;
				this.isPrivacy = false;
				this.isAgentWork = false;
				this.isAbout = false;
				this.isNotification = false;
				this.isProfile = true;
				this.isDevice = false;
				this.isTheme = false;
				this.isDesktopSection = false;
				this.isExperimental = false;
				this.updateQueryParam('profile');
			},

			showDeviceSettings() {
				this.isLocalData = false;
				this.isPrivacy = false;
				this.isAgentWork = false;
				this.isAbout = false;
				this.isNotification = false;
				this.isProfile = false;
				this.isDevice = true;
				this.isTheme = false;
				this.isDesktopSection = false;
				this.isExperimental = false;
				this.updateQueryParam('device');
			},

			showThemeSettings() {
				this.isLocalData = false;
				this.isPrivacy = false;
				this.isAgentWork = false;
				this.isAbout = false;
				this.isNotification = false;
				this.isProfile = false;
				this.isDevice = false;
				this.isTheme = true;
				this.isDesktopSection = false;
				this.isExperimental = false;
				this.updateQueryParam('theme');
			},

			showDesktopSettings() {
				this.isLocalData = false;
				this.isPrivacy = false;
				this.isAgentWork = false;
				this.isAbout = false;
				this.isNotification = false;
				this.isProfile = false;
				this.isDevice = false;
				this.isTheme = false;
				this.isDesktopSection = true;
				this.isExperimental = false;
				this.updateQueryParam('desktop');
			},

			showExperimentalSettings() {
				this.isLocalData = false;
				this.isPrivacy = false;
				this.isAgentWork = false;
				this.isAbout = false;
				this.isNotification = false;
				this.isProfile = false;
				this.isDevice = false;
				this.isTheme = false;
				this.isDesktopSection = false;
				this.isExperimental = true;
				this.updateQueryParam('experimental');
			},

			showLocalDataSettings() {
				this.isNotification = false;
				this.isProfile = false;
				this.isDevice = false;
				this.isTheme = false;
				this.isDesktopSection = false;
				this.isExperimental = false;
				this.isLocalData = true;
				this.isPrivacy = false;
				this.isAgentWork = false;
				this.isAbout = false;
				this.updateQueryParam('data');
			},

			async loadAppVersion() {
				if (!this.isDesktop || this.appVersion) return;
				try {
					const { getVersion } = await import('@tauri-apps/api/app');
					this.appVersion = await getVersion();
				} catch {
					this.appVersion = '';
				}
			},

			showPrivacySettings() {
				this.isNotification = false;
				this.isProfile = false;
				this.isDevice = false;
				this.isTheme = false;
				this.isDesktopSection = false;
				this.isExperimental = false;
				this.isLocalData = false;
				this.isAbout = false;
				this.isAgentWork = false;
				this.isPrivacy = true;
				this.updateQueryParam('privacy');
			},

			showAgentWorkSettings() {
				this.isNotification = false;
				this.isProfile = false;
				this.isDevice = false;
				this.isTheme = false;
				this.isDesktopSection = false;
				this.isExperimental = false;
				this.isLocalData = false;
				this.isAbout = false;
				this.isPrivacy = false;
				this.isAgentWork = true;
				this.updateQueryParam('agent-work');
			},

			showAboutSettings() {
				this.loadAppVersion();
				this.isNotification = false;
				this.isProfile = false;
				this.isDevice = false;
				this.isTheme = false;
				this.isDesktopSection = false;
				this.isExperimental = false;
				this.isLocalData = false;
				this.isPrivacy = false;
				this.isAgentWork = false;
				this.isAbout = true;
				this.updateQueryParam('about');
			},

			updateQueryParam(tab) {
				this.$router
					.push({
						query: { ...this.$route.query, tab },
					})
					.catch(() => {});
			},

			onPasswordChanged() {
				this.user = {
					...this.user,
					has_smart_device_token: false,
					smart_device_token: null,
				};
				this.freshToken = null;
			},
			async generateSmartDeviceToken() {
				this.showConfirm(
					t('settings.device.generateTitle'),
					this.hasSmartDeviceToken
						? t('settings.device.regenerateBody')
						: t('settings.device.generateBody'),
					async () => {
						let response;
						try {
							response = await generateSmartDeviceToken();
						} catch (error) {
							console.error('Failed to generate token:', error);
							this.confirm = null;
							this.showAlert(
								serverMessageFrom(error, t('settings.device.generateFailed')),
								'error',
							);
							return;
						}
						this.freshToken = response?.data?.token ?? null;
						this.showToken = false;
						this.confirm = null;
						this.showAlert(t('settings.device.generated'));
						await this.refreshUserAfterTokenChange();
					},
				);
			},

			async revokeSmartDeviceToken() {
				this.showConfirm(
					t('settings.device.revokeTitle'),
					t('settings.device.revokeBody'),
					async () => {
						try {
							await revokeSmartDeviceToken();
						} catch (error) {
							console.error('Failed to revoke token:', error);
							this.confirm = null;
							this.showAlert(
								serverMessageFrom(error, t('settings.device.revokeFailed')),
								'error',
							);
							return;
						}
						this.freshToken = null;
						this.user = {
							...this.user,
							has_smart_device_token: false,
							smart_device_token: null,
						};
						this.confirm = null;
						this.showAlert(t('settings.device.revoked'));
						await this.refreshUserAfterTokenChange();
					},
				);
			},

			async refreshUserAfterTokenChange() {
				try {
					this.user = await getUser();
				} catch (error) {
					console.error('Failed to refresh user:', error);
				}
			},

			showAlert(message, type = 'success') {
				if (type === 'error') {
					this.toaster.toast({
						title: t('settings.alerts.errorTitle'),
						description: message,
						variant: 'destructive',
					});
					return;
				}
				this.toaster.toast({
					title: message,
					action: CircleCheckBigIcon,
					class: 'bg-green-500 border-0 text-white',
				});
			},

			async generateTelegramLink() {
				try {
					const link = await generateLink();

					this.showConfirm(
						t('settings.telegram.connect'),
						t('settings.telegram.connectBody'),
						() => {
							window.open(link, '_blank');
							this.confirm = null;
						},
						link,
					);
				} catch (error) {
					console.error('Failed to generate Telegram link:', error);
				}
			},

			async unlinkTelegram() {
				this.showConfirm(
					t('settings.telegram.unlinkTitle'),
					t('settings.telegram.unlinkBody'),
					async () => {
						try {
							await unlink();
							this.user = await getUser();
							this.confirm = null;
						} catch (error) {
							console.error('Failed to unlink Telegram:', error);
						}
					},
				);
			},

			showConfirm(title, body, action, link = null) {
				this.confirm = { title, body, action, link };
			},

			async testWebPushNotifications() {
				await sendNotification();
			},

			async togglePushes() {
				if (this.$store.getters.getPusherBeamsUserId) {
					writeNtfySubscribed(localStorage, this.$store.state.user?.id, false);
					await this.$store.getters.getPusherBeamsClient.stop();
					return this.$store.commit('setPusherBeamsUserId', null);
				}
				const userId = this.$store.state.user?.id.toString();

				try {
					await this.$store.getters.getPusherBeamsClient.start();
					await this.$store.getters.getPusherBeamsClient.setUserId(
						userId,
						this.$store.getters.getPusherTokenProvider,
					);
					writeNtfySubscribed(localStorage, userId, true);
					this.$store.commit('setPusherBeamsUserId', userId);
				} catch (e) {
					this.showConfirm(
						t('settings.push.registrationTitle'),
						t('settings.push.registrationBody'),
						() => {
							this.confirm = null;
						},
					);
				}
			},

			async loadSettings() {
				const data = await getUserSettingsV2();
				this.initSettings(data, this.user.settings);
				this.availableSettings = data;
			},

			initSettings(availableSettings, settings = []) {
				return availableSettings.map((item, index) => {
					const setting = this.getSettingById(settings, item.id, {
						id: item.id,
						value: '',
					});
					this.settings[index] = setting;
					item.show_custom_value_input =
						item.default_values &&
						item.default_values.findIndex(
							(val) => val.value === setting.value,
						) === -1;
				});
			},

			getSettingById(settings, id, defaultResult = null) {
				return settings.find((setting) => setting.id === id) || defaultResult;
			},

			async updateSettings() {
				if (this.savingSettings) return;
				this.savingSettings = true;
				try {
					// Store original editor value to check if it changes
					const originalEditorSetting = this.settings.find(
						(setting) => setting.key === 'preferred_editor',
					);
					const originalEditorValue = originalEditorSetting?.value;

					const [data] = await Promise.all([
						updateUserSettingsV2(withCurrentAgentUsageSync(this.settings)),
						updateUserSettings({ settings: this.userSettings }),
					]);

					// Find and save preferred_editor to localStorage with improved reliability
					const preferredEditorSetting = this.settings.find(
						(setting) => setting.key === 'preferred_editor',
					);
					if (preferredEditorSetting) {
						// Make sure we convert to string and handle null/undefined values
						let editorValue = preferredEditorSetting.value
							? String(preferredEditorSetting.value).toLowerCase().trim()
							: '';

						// Validate the editor value
						if (
							editorValue &&
							['block', 'markdown', 'blockmd'].includes(editorValue)
						) {
							console.log(
								'Saving editor preference to localStorage:',
								editorValue,
							);
							localStorage.setItem('preferred_editor', editorValue);

							// Also update the editor value in the Vuex store
							if (this.$store.state.user && this.$store.state.user.settings) {
								// Update the store's user settings directly
								this.$store.state.user.settings =
									this.$store.state.user.settings.map((setting) => {
										if (setting.key === 'preferred_editor') {
											return { ...setting, value: editorValue };
										}
										return setting;
									});
							}

							// If editor setting has changed, reload the page to apply changes immediately
							const normalizedOriginalValue = originalEditorValue
								? String(originalEditorValue).toLowerCase().trim()
								: '';

							if (normalizedOriginalValue !== editorValue) {
								this.showAlert(t('settings.alerts.editorUpdated'));

								// Force localStorage update one more time to ensure it's saved before reload
								localStorage.setItem('preferred_editor', editorValue);

								setTimeout(() => {
									window.location.reload();
								}, 500);
								return;
							}
						} else if (editorValue) {
							console.error('Invalid editor value:', editorValue);
						}
					}

					this.initSettings(this.availableSettings, data.settings);
					this.showAlert(t('settings.alerts.updated'));
				} catch (e) {
					console.error(e);
					this.showAlert(t('settings.alerts.updateFailed'), 'error');
				} finally {
					this.savingSettings = false;
				}
			},
		},
	};
</script>

<style scoped>
	.settings-container {
		max-width: 700px;
		margin: 50px auto;
		padding: 20px;
		box-shadow: rgb(233 233 233) 1px 4px 20px;
	}
</style>
