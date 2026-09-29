<template>
	<component
		:is="standalone ? 'PageContainer' : 'div'"
		:width="standalone ? 'narrow' : undefined"
	>
		<PageHeader v-if="standalone" title="Profile" />
		<div class="flex flex-col gap-6">
			<SettingsSection title="Profile picture">
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
								{{ avatarBusy ? 'Uploading…' : 'Change picture' }}
							</Button>
							<Button
								v-if="user.has_avatar"
								variant="ghost"
								size="sm"
								:disabled="avatarBusy"
								@click="dropAvatar"
							>
								Remove
							</Button>
						</div>
						<p v-if="avatarError" class="text-xs text-destructive">
							{{ avatarError }}
						</p>
						<p class="text-xs text-ink-subtle">
							Accepted formats: PNG, JPEG, WebP, GIF.
						</p>
					</div>
				</div>
			</SettingsSection>

			<SettingsSection title="Account">
				<div class="flex max-w-sm flex-col gap-4">
					<div class="flex flex-col gap-1.5">
						<Label for="profile-name">Name</Label>
						<Input
							id="profile-name"
							v-model="user.name"
							autocomplete="name"
						/>
						<p v-if="errors.name" class="text-xs text-destructive">
							{{ errors.name[0] }}
						</p>
					</div>
					<div v-if="user.email" class="flex flex-col gap-1.5">
						<Label for="profile-email">Email</Label>
						<Input id="profile-email" :model-value="user.email" disabled />
						<p class="text-xs text-ink-subtle">
							Email can't be changed here
						</p>
					</div>
				</div>
				<div class="mt-6 border-t border-border pt-6">
					<h3 class="text-sm font-semibold text-ink">Change password</h3>
					<p class="mt-1 text-sm text-ink-subtle">
						Leave blank to keep your current password.
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
					<div class="flex flex-col gap-1.5">
						<Label for="profile-password">New password</Label>
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
							Confirm password
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
				</div>
				<template #footer>
					<Button :disabled="saving" @click="saveUser">
						{{ saving ? 'Saving…' : 'Save' }}
					</Button>
				</template>
			</SettingsSection>

			<SettingsSection
				tone="danger"
				title="Agent connections"
				description="Revoke every persona token, across all your personas and workspaces."
			>
				<template #actions>
					<Button
						variant="destructive"
						size="sm"
						:disabled="revokingAllTokens"
						@click="revokeAllPersonaTokens"
					>
						{{ revokingAllTokens ? 'Revoking…' : 'Revoke all persona tokens' }}
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
	import UserAvatar from '@/components/general/UserAvatar.vue';
	import PageContainer from '@/components/layouts/PageContainer.vue';
	import PageHeader from '@/components/layouts/PageHeader.vue';
	import SettingsSection from '@/components/layouts/SettingsSection.vue';
	import { Button } from '@/components/ui/button';
	import { Input } from '@/components/ui/input';
	import { Label } from '@/components/ui/label';
	import { toast } from '@/components/ui/toast';
	import { setDocumentTitle } from '@/composable/useDocumentTitle';

	export default {
		name: 'Profile',
		components: {
			UserAvatar,
			PageContainer,
			PageHeader,
			SettingsSection,
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
		data: () => ({
			user: {
				name: null,
				password: null,
				password_confirmation: null,
			},
			errors: {},
			avatarBusy: false,
			avatarError: null,
			revokingAllTokens: false,
			saving: false,
			tokensRevoked: false,
			// Bumped after an upload so the avatar re-reads its link instead of the cached one.
			avatarKey: 0,
		}),
		async mounted() {
			setDocumentTitle('Profile');
			// @todo try to get from store this data first
			this.user = await getUser(); // inside we put response to store. @todo think how to reorganize it or don't care
		},
		computed: {
			tokensRevokedNotice: () =>
				'Device, agent and notification tokens were revoked — generate new ones.',
		},
		methods: {
			async onAvatarPicked(event) {
				const file = event.target.files?.[0];
				event.target.value = '';

				if (!file) {
					return;
				}

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
						error.response?.data?.message ?? 'Could not upload that picture';
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
						error.response?.data?.message ?? 'Could not remove the picture';
				} finally {
					this.avatarBusy = false;
				}
			},
			async saveUser() {
				this.saving = true;
				const passwordChanged = !!this.user.password;
				try {
					const updated = await updateUser(this.user);
					if (updated && typeof updated === 'object')
						this.user = { ...this.user, ...updated };
					this.user.password = null;
					this.user.password_confirmation = null;
					this.errors = {};
					if (passwordChanged) {
						this.tokensRevoked = true;
						this.$emit('password-changed');
						toast({
							title: 'Password changed',
							description: this.tokensRevokedNotice,
						});
					} else {
						toast({ title: 'Saved', description: 'User data saved' });
					}
				} catch (error) {
					this.errors = error.response?.data?.errors ?? {};
				} finally {
					this.saving = false;
				}
			},
			async revokeAllPersonaTokens() {
				if (
					!window.confirm(
						'Revoke every persona token across all your personas?',
					)
				)
					return;
				this.revokingAllTokens = true;
				try {
					await revokeAllMyPersonaTokens();
					toast({ title: 'Done', description: 'All persona tokens revoked' });
				} catch {
					toast({
						title: 'Error',
						description: 'Could not revoke persona tokens',
						variant: 'destructive',
					});
				} finally {
					this.revokingAllTokens = false;
				}
			},
		},
	};
</script>
