<template>
	<div>
		<BaseLayout width="narrow" :title="pageTitle">
			<template #body>
				<AsyncContent
					:pending="initialPending"
					:loaded="initialLoaded"
					:error="initialError"
					:retry="loadInitialSettings"
					label="Loading settings"
				>
					<div class="flex flex-col gap-6">
						<div v-if="isNotification" class="flex flex-col gap-6">
							<NotificationSettingsForm />

							<SettingsSection title="Push notifications (legacy)">
								<div class="flex flex-col gap-4">
									<div class="flex flex-wrap gap-2">
										<Button
											v-if="!pusherBeamsUserId"
											variant="outline"
											size="sm"
											@click="togglePushes"
										>
											Enable web pushes
										</Button>
										<Button
											variant="outline"
											size="sm"
											@click="testWebPushNotifications"
										>
											Send test notification
										</Button>
									</div>
									<SettingsRow
										v-slot="{ labelId, descriptionId }"
										label="Show tooltips"
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
										Save
									</Button>
								</template>
							</SettingsSection>
						</div>

						<SettingsSection
							v-if="isDesktopSection && isDesktop"
							title="Keyboard shortcuts"
						>
							<DesktopShortcutsSettings />
						</SettingsSection>

						<SettingsSection v-if="isTheme" title="Theme" description="Pick a color theme and light/dark mode.">
							<ThemePicker />
						</SettingsSection>

						<div v-if="isProfile" class="flex flex-col gap-6">
							<profile :standalone="false" />

							<SettingsSection title="Telegram">
								<div
									v-if="user.telegram_username"
									class="flex flex-wrap items-center gap-3"
								>
									<span class="text-sm text-status-done">
										Connected as @{{ user.telegram_username }}
									</span>
									<Button
										variant="outline"
										size="sm"
										class="text-destructive"
										@click="unlinkTelegram"
									>
										Unlink
									</Button>
								</div>
								<Button v-else @click="generateTelegramLink">
									Connect Telegram
								</Button>
							</SettingsSection>
						</div>

						<div v-if="isDevice" class="flex flex-col gap-6">
							<SettingsSection title="API token">
								<div class="flex max-w-xl flex-col gap-1.5">
									<Label for="smart-device-token">API token</Label>
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
													? 'Token is set (hidden). Generate a new one if you lost it.'
													: 'Token needs to be generated'
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
											{{ tokenCopied ? 'Copied' : 'Copy' }}
										</Button>
										<Button
											v-if="tokenState === 'fresh'"
											variant="outline"
											size="sm"
											@click="showToken = !showToken"
										>
											<EyeOff v-if="showToken" />
											<Eye v-else />
											{{ showToken ? 'Hide' : 'Show' }}
										</Button>
									</div>
									<p v-if="tokenState === 'fresh'" class="text-xs text-ink-subtle">
										Copy it now — it won't be shown again.
									</p>
									<p v-if="hasSmartDeviceToken" class="text-xs text-ink-subtle">
										Created: {{ formatDate(user.smart_device_token_created_at) }}
									</p>
								</div>
								<template #footer>
									<Button @click="generateSmartDeviceToken">
										{{
											hasSmartDeviceToken
												? 'Generate new token'
												: 'Generate token'
										}}
									</Button>
									<Button
										v-if="hasSmartDeviceToken"
										variant="destructive"
										@click="revokeSmartDeviceToken"
									>
										Revoke token
									</Button>
								</template>
							</SettingsSection>

							<SettingsSection title="How to use">
								<div class="flex flex-col gap-2 text-sm text-ink-subtle">
									<p>1. Generate a token using the button above</p>
									<p>
										2. Include the token in your device's API requests using
										the header:
									</p>
									<code
										class="block overflow-x-auto rounded-md border border-border bg-muted px-3 py-2 font-mono text-xs text-ink"
									>
										X-Smart-Device-Token: your_token_here
									</code>
									<p>
										Keep your token secure. If compromised, generate a new one
										immediately.
									</p>
								</div>
							</SettingsSection>

							<SettingsSection title="MCP setup">
								<div class="flex flex-col gap-3 text-sm text-ink-subtle">
									<p>
										TMGR MCP uses the smart device token as a custom header.
										Generate the token above, then configure an MCP client
										that supports remote Streamable HTTP servers with headers
										(Claude Code:
										<code
											class="rounded border border-border bg-muted px-1.5 py-0.5 font-mono text-xs text-ink"
											>claude mcp add --transport http tmgr
											{{ mcpUrl }} --header "X-Smart-Device-Token:
											&lt;token&gt;"</code
										>).
									</p>
									<div>
										<p class="mb-1 font-medium text-ink">Server URL</p>
										<code
											class="block overflow-x-auto rounded-md border border-border bg-muted px-3 py-2 font-mono text-xs text-ink"
										>
											{{ mcpUrl }}
										</code>
									</div>
									<div>
										<p class="mb-1 font-medium text-ink">
											Client configuration
										</p>
										<pre
											class="overflow-x-auto whitespace-pre rounded-md border border-border bg-muted px-3 py-2 font-mono text-xs text-ink"
											>{{ mcpClientConfig }}</pre
										>
									</div>
									<p>
										OAuth note: this TMGR MCP server is not an OAuth connector
										yet. Unlike Spendly, it authenticates `/mcp/**` with
										`X-Smart-Device-Token`, so Claude.ai custom connectors that
										require OAuth discovery are not supported by this setup.
									</p>
								</div>
							</SettingsSection>

							<SettingsSection title="Documentation">
								<ul class="flex flex-col gap-1 text-sm">
									<li>
										<a
											:href="`${docsBaseUrl}/docs/smart-devices.html`"
											target="_blank"
											rel="noopener"
											class="text-primary hover:underline"
										>
											Smart Device API reference
										</a>
									</li>
									<li>
										<a
											:href="`${docsBaseUrl}/docs/mcp.html`"
											target="_blank"
											rel="noopener"
											class="text-primary hover:underline"
										>
											MCP server setup
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
								Open Telegram
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
	import Confirm from '@/components/general/Confirm.vue';
	import ThemePicker from '@/components/general/ThemePicker.vue';
	import DesktopShortcutsSettings from '@/components/desktop/DesktopShortcutsSettings.vue';
	import SettingsRow from '@/components/layouts/SettingsRow.vue';
	import SettingsSection from '@/components/layouts/SettingsSection.vue';
	import NotificationSettingsForm from '@/components/notifications/NotificationSettingsForm.vue';
	import {
		BreadcrumbItem,
		BreadcrumbLink,
		BreadcrumbSeparator,
	} from '@/components/ui/breadcrumb';
	import { Button } from '@/components/ui/button';
	import { Input } from '@/components/ui/input';
	import { Label } from '@/components/ui/label';
	import { Switch } from '@/components/ui/switch';
	import { setDocumentTitle } from '@/composable/useDocumentTitle';
	import Profile from '@/pages/Profile.vue';
	import { isDesktopApp } from '@/utils/desktop';
	import { hasSmartDeviceToken, smartDeviceTokenState } from '@/utils/smartDeviceToken';
	import { Check, Copy, Eye, EyeOff } from 'lucide-vue-next';

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
			DesktopShortcutsSettings,
			ThemePicker,
			SettingsSection,
			SettingsRow,
			Button,
			Input,
			Label,
			Switch,
			Check,
			Copy,
			Eye,
			EyeOff,
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
			pageTitle() {
				if (this.isProfile) return 'Profile';
				if (this.isDevice) return 'Smart devices';
				if (this.isNotification) return 'Notifications';
				if (this.isTheme) return 'Theme';
				if (this.isDesktopSection) return 'Keyboard shortcuts';
				return 'Settings';
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
				const apiBaseUrl = import.meta.env.VITE_API_BASE_URL || '/api/';

				try {
					const url = new URL(apiBaseUrl, window.location.origin);
					const basePath = url.pathname
						.replace(/\/api\/?$/, '')
						.replace(/\/$/, '');

					url.pathname = `${basePath}/mcp`;
					url.search = '';
					url.hash = '';

					return url.toString();
				} catch (error) {
					return 'https://api.tmgr.dev/mcp';
				}
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
			setDocumentTitle('Settings');
			await this.loadInitialSettings();
		},
		methods: {
			async loadInitialSettings() {
				this.initialPending = true;
				this.initialError = null;
				try {
					const [user, settings] = await Promise.all([
						getUser(),
						getUserSettingsV2(),
					]);
					this.user = user;
					this.initSettings(settings, user.settings);
					this.availableSettings = settings;
					this.initialLoaded = true;
				} catch {
					this.initialError = 'Could not load settings.';
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
							this.showNotificationSettings();
							break;
						case 'workspace':
							this.openWorkspaceSettings();
							break;
						case 'profile':
							this.showProfileSettings();
							break;
						case 'device':
							this.showDeviceSettings();
							break;
						case 'theme':
							this.showThemeSettings();
							break;
						case 'desktop':
							this.showDesktopSettings();
							break;
					}
				}
			},

			showNotificationSettings() {
				this.isProfile = false;
				this.isNotification = true;
				this.isDevice = false;
				this.isTheme = false;
				this.isDesktopSection = false;
				this.updateQueryParam('notification');
			},

			openWorkspaceSettings() {
				this.$router.replace('/settings/workspaces').catch(() => {});
			},

			showProfileSettings() {
				this.isNotification = false;
				this.isProfile = true;
				this.isDevice = false;
				this.isTheme = false;
				this.isDesktopSection = false;
				this.updateQueryParam('profile');
			},

			showDeviceSettings() {
				this.isNotification = false;
				this.isProfile = false;
				this.isDevice = true;
				this.isTheme = false;
				this.isDesktopSection = false;
				this.updateQueryParam('device');
			},

			showThemeSettings() {
				this.isNotification = false;
				this.isProfile = false;
				this.isDevice = false;
				this.isTheme = true;
				this.isDesktopSection = false;
				this.updateQueryParam('theme');
			},

			showDesktopSettings() {
				this.isNotification = false;
				this.isProfile = false;
				this.isDevice = false;
				this.isTheme = false;
				this.isDesktopSection = true;
				this.updateQueryParam('desktop');
			},

			updateQueryParam(tab) {
				this.$router
					.push({
						query: { ...this.$route.query, tab },
					})
					.catch(() => {});
			},

			async generateSmartDeviceToken() {
				this.showConfirm(
					'Generate New Token',
					this.hasSmartDeviceToken
						? 'Are you sure you want to generate a new token? This will invalidate the existing token and any devices using it will need to be updated.'
						: 'Generate a new token for your smart devices?',
					async () => {
						try {
							const response = await generateSmartDeviceToken();
							this.freshToken = response?.data?.token ?? null;
							this.showToken = false;
							this.user = await getUser();
							this.confirm = null;
							this.showAlert('Token generated successfully');
						} catch (error) {
							console.error('Failed to generate token:', error);
							this.showAlert('Failed to generate token', 'error');
						}
					},
				);
			},

			async revokeSmartDeviceToken() {
				this.showConfirm(
					'Revoke Token',
					'Are you sure you want to revoke this token? All devices using this token will stop working until reconfigured with a new token.',
					async () => {
						try {
							await revokeSmartDeviceToken();
							this.freshToken = null;
							this.user = await getUser();
							this.confirm = null;
							this.showAlert('Token revoked successfully');
						} catch (error) {
							console.error('Failed to revoke token:', error);
							this.showAlert('Failed to revoke token', 'error');
						}
					},
				);
			},

			showAlert(message, type = 'success') {
				// Implement your alert system here
				// This could be a toast notification or any other alert mechanism
				console.log(`${type}: ${message}`);
			},

			async generateTelegramLink() {
				try {
					const link = await generateLink();

					this.showConfirm(
						'Connect Telegram',
						"Click the button below to open Telegram and connect your account. After connecting, you'll receive notifications through Telegram.",
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
					'Unlink Telegram',
					'Are you sure you want to unlink your Telegram account? You will no longer receive notifications through Telegram.',
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
					this.$store.commit('setPusherBeamsUserId', userId);
				} catch (e) {
					this.showConfirm(
						'Notifications registration',
						'Please check notifications permissions',
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
						updateUserSettingsV2(this.settings),
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
								this.showAlert('Editor updated. Reloading page...');

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
					this.showAlert('Settings updated successfully');
				} catch (e) {
					console.error(e);
					this.showAlert('Failed to update settings', 'error');
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
