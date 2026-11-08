<template>
	<component
		:is="standalone ? 'PageContainer' : 'div'"
		:width="standalone ? 'narrow' : undefined"
	>
		<PageHeader v-if="standalone" :title="$t('settingsPages.profile.title')" />
		<div class="flex flex-col gap-6">
			<SettingsSection :title="$t('settingsPages.profile.pictureTitle')">
				<div class="flex items-center gap-4">
					<UserAvatar
						:key="avatarKey"
						:user-id="user.id ?? 0"
						:name="user.name ?? ''"
						:has-avatar="user.has_avatar ?? false"
						:size="72"
					/>
					<div class="flex flex-col gap-2">
						<input
							ref="avatarInput"
							type="file"
							accept="image/png,image/jpeg,image/webp,image/gif"
							class="hidden"
							@change="onAvatarPicked"
						/>
						<div class="flex flex-wrap gap-2">
							<Button
								variant="outline"
								size="sm"
								:disabled="avatarBusy"
								@click="$refs.avatarInput.click()"
							>
								{{
									avatarBusy
										? $t('settingsPages.profile.uploading')
										: $t('settingsPages.profile.changePicture')
								}}
							</Button>
							<Button
								v-if="user.has_avatar"
								variant="ghost"
								size="sm"
								:disabled="avatarBusy"
								@click="dropAvatar"
							>
								{{ $t('common.actions.remove') }}
							</Button>
						</div>
						<AvatarCropModal
							v-if="cropFile"
							:file="cropFile"
							@confirm="uploadAvatar"
							@cancel="cropFile = null"
							@error="onCropError"
						/>
						<p v-if="avatarError" class="text-xs text-destructive">
							{{ avatarError }}
						</p>
						<p class="text-xs text-ink-subtle">
							{{ $t('settingsPages.profile.acceptedFormats') }}
						</p>
					</div>
				</div>
			</SettingsSection>

			<SettingsSection :title="$t('settingsPages.profile.accountTitle')">
				<div class="flex max-w-sm flex-col gap-4">
					<div class="flex flex-col gap-1.5">
						<Label for="profile-name">{{
							$t('settingsPages.profile.name')
						}}</Label>
						<Input id="profile-name" v-model="user.name" autocomplete="name" />
						<p v-if="errors.name" class="text-xs text-destructive">
							{{ errors.name[0] }}
						</p>
					</div>
					<div v-if="user.email" class="flex flex-col gap-1.5">
						<Label for="profile-email">{{
							$t('settingsPages.profile.email')
						}}</Label>
						<Input id="profile-email" :model-value="user.email" disabled />
						<p class="text-xs text-ink-subtle">
							{{ $t('settingsPages.profile.emailReadonly') }}
						</p>
					</div>
				</div>
				<div class="mt-6 border-t border-border pt-6">
					<h3 class="text-sm font-semibold text-ink">
						{{ $t('settingsPages.profile.changePassword') }}
					</h3>
					<p class="mt-1 text-sm text-ink-subtle">
						{{ $t('settingsPages.profile.leaveBlank') }}
					</p>
					<p
						v-if="tokensRevoked"
						class="mt-2 text-sm text-ink-subtle"
						data-testid="tokens-revoked-notice"
					>
						{{ tokensRevokedNotice }}
					</p>
				</div>
				<div class="mt-4 grid grid-cols-1 gap-4 sm:grid-cols-2">
					<div class="flex flex-col gap-1.5 sm:col-span-2">
						<Label for="profile-current-password">{{
							$t('settingsPages.profile.currentPassword')
						}}</Label>
						<Input
							id="profile-current-password"
							v-model="user.current_password"
							type="password"
							autocomplete="current-password"
						/>
						<p
							v-if="errors?.current_password"
							class="text-xs text-destructive"
							data-testid="current-password-error"
						>
							{{ errors.current_password[0] }}
						</p>
					</div>
					<div class="flex flex-col gap-1.5">
						<Label for="profile-password">{{
							$t('settingsPages.profile.newPassword')
						}}</Label>
						<Input
							id="profile-password"
							v-model="user.password"
							type="password"
							autocomplete="new-password"
						/>
						<p v-if="errors?.password" class="text-xs text-destructive">
							{{ errors.password[0] }}
						</p>
					</div>
					<div class="flex flex-col gap-1.5">
						<Label for="profile-password-confirmation">
							{{ $t('settingsPages.profile.confirmPassword') }}
						</Label>
						<Input
							id="profile-password-confirmation"
							v-model="user.password_confirmation"
							type="password"
							autocomplete="new-password"
						/>
						<p
							v-if="errors.password_confirmation"
							class="text-xs text-destructive"
						>
							{{ errors.password_confirmation[0] }}
						</p>
					</div>
					<p class="text-xs text-ink-subtle sm:col-span-2">
						{{ $t('settingsPages.profile.socialHint') }}
					</p>
				</div>
				<label
					for="profile-logout-other-sessions"
					class="mt-4 flex items-start gap-2 text-sm text-ink"
				>
					<input
						id="profile-logout-other-sessions"
						v-model="logoutOtherSessions"
						type="checkbox"
						class="mt-0.5 accent-primary"
						data-testid="logout-other-sessions"
					/>
					<span>
						{{ $t('settingsPages.profile.signOutOthers') }}
						<span class="block text-xs text-ink-subtle">
							{{ $t('settingsPages.profile.signOutOthersHint') }}
						</span>
					</span>
				</label>
				<template #footer>
					<Button :disabled="saving" @click="saveUser">
						{{
							saving ? $t('common.actions.saving') : $t('common.actions.save')
						}}
					</Button>
				</template>
			</SettingsSection>

			<SignedInDevices />

			<SettingsSection
				tone="danger"
				:title="$t('settingsPages.profile.agentTitle')"
				:description="$t('settingsPages.profile.agentDescription')"
			>
				<template #actions>
					<Button
						variant="destructive"
						size="sm"
						:disabled="revokingAllTokens"
						@click="revokeAllPersonaTokens"
					>
						{{
							revokingAllTokens
								? $t('settingsPages.profile.revoking')
								: $t('settingsPages.profile.revokeAll')
						}}
					</Button>
				</template>
			</SettingsSection>
		</div>
	</component>
</template>

<script>
	import {
		forgetAvatar,
		removeAvatar,
		storeAvatar,
	} from '@/actions/tmgr/avatars';
	import { presignUpload, putToStorage } from '@/actions/tmgr/files';
	import { revokeAllMyPersonaTokens } from '@/actions/tmgr/personas';
	import { getUser, updateUser } from '@/actions/tmgr/user';
	import SignedInDevices from '@/components/devices/SignedInDevices.vue';
	import UserAvatar from '@/components/general/UserAvatar.vue';
	import PageContainer from '@/components/layouts/PageContainer.vue';
	import PageHeader from '@/components/layouts/PageHeader.vue';
	import SettingsSection from '@/components/layouts/SettingsSection.vue';
	import AvatarCropModal from '@/components/profile/AvatarCropModal.vue';
	import { Button } from '@/components/ui/button';
	import { Input } from '@/components/ui/input';
	import { Label } from '@/components/ui/label';
	import { toast } from '@/components/ui/toast';
	import { setDocumentTitle } from '@/composable/useDocumentTitle';
	import { t } from '@/i18n';

	export default {
		name: 'Profile',
		components: {
			UserAvatar,
			AvatarCropModal,
			PageContainer,
			PageHeader,
			SettingsSection,
			SignedInDevices,
			Button,
			Input,
			Label,
		},
		props: {
			standalone: {
				type: Boolean,
				default: true,
			},
		},
		emits: ['password-changed'],
		data: () => ({
			user: {
				name: null,
				password: null,
				password_confirmation: null,
				current_password: null,
			},
			errors: {},
			avatarBusy: false,
			avatarError: null,
			cropFile: null,
			revokingAllTokens: false,
			saving: false,
			logoutOtherSessions: true,
			tokensRevoked: false,
			// Bumped after an upload so the avatar re-reads its link instead of the cached one.
			avatarKey: 0,
		}),
		async mounted() {
			setDocumentTitle(t('settingsPages.profile.title'));
			// @todo try to get from store this data first
			this.user = await getUser(); // inside we put response to store. @todo think how to reorganize it or don't care
		},
		computed: {
			tokensRevokedNotice: () => t('settingsPages.profile.tokensRevokedNotice'),
		},
		methods: {
			async onAvatarPicked(event) {
				const file = event.target.files?.[0];
				event.target.value = '';

				if (!file) {
					return;
				}

				this.avatarError = null;

				if (!file.type.startsWith('image/')) {
					this.avatarError = t('settingsPages.profile.pickImage');
					return;
				}

				this.cropFile = file;
			},
			onCropError() {
				this.cropFile = null;
				this.avatarError = t('settingsPages.profile.readFailed');
			},
			async uploadAvatar(file) {
				this.cropFile = null;
				this.avatarBusy = true;
				this.avatarError = null;

				try {
					// The same presign the attachments use; only the claim below is avatar-specific.
					const target = await presignUpload(file);
					await putToStorage(target, file);
					await storeAvatar(target.key);
					this.user = { ...this.user, has_avatar: true };
					forgetAvatar(this.user.id);
					this.avatarKey += 1;
				} catch (error) {
					this.avatarError =
						error.response?.data?.message ??
						t('settingsPages.profile.uploadFailed');
				} finally {
					this.avatarBusy = false;
				}
			},
			async dropAvatar() {
				this.avatarBusy = true;
				this.avatarError = null;

				try {
					await removeAvatar();
					this.user = { ...this.user, has_avatar: false };
					forgetAvatar(this.user.id);
					this.avatarKey += 1;
				} catch (error) {
					this.avatarError =
						error.response?.data?.message ??
						t('settingsPages.profile.removeFailed');
				} finally {
					this.avatarBusy = false;
				}
			},
			async saveUser() {
				this.saving = true;
				const passwordChanged = !!this.user.password;
				try {
					const updated = await updateUser(this.user, {
						logoutOtherSessions: this.logoutOtherSessions,
					});
					if (updated && typeof updated === 'object')
						this.user = { ...this.user, ...updated };
					this.user.password = null;
					this.user.password_confirmation = null;
					this.user.current_password = null;
					this.errors = {};
					if (passwordChanged) {
						this.tokensRevoked = true;
						this.$emit('password-changed');
						toast({
							title: t('settingsPages.profile.passwordChanged'),
							description: this.tokensRevokedNotice,
						});
					} else {
						toast({
							title: t('settingsPages.profile.savedTitle'),
							description: t('settingsPages.profile.savedDescription'),
						});
					}
				} catch (error) {
					const data = error.response?.data;
					this.errors = data?.errors ?? {};
					if (
						!this.errors.current_password &&
						/current password/i.test(data?.message ?? '')
					)
						this.errors = { ...this.errors, current_password: [data.message] };
				} finally {
					this.saving = false;
				}
			},
			async revokeAllPersonaTokens() {
				if (!window.confirm(t('settingsPages.profile.confirmRevokeAll')))
					return;
				this.revokingAllTokens = true;
				try {
					await revokeAllMyPersonaTokens();
					toast({
						title: t('settingsPages.profile.doneTitle'),
						description: t('settingsPages.profile.allRevoked'),
					});
				} catch {
					toast({
						title: t('settingsPages.profile.errorTitle'),
						description: t('settingsPages.profile.revokeFailed'),
						variant: 'destructive',
					});
				} finally {
					this.revokingAllTokens = false;
				}
			},
		},
	};
</script>
