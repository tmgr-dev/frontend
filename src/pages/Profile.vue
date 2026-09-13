<template>
	<div>
		<div class="flex max-w-lg flex-col gap-3">
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
					<button
						type="button"
						class="rounded border border-line px-3 py-1.5 text-sm text-ink hover:bg-surface-sunken"
						:disabled="avatarBusy"
						@click="$refs.avatarInput.click()"
					>
						{{ avatarBusy ? 'Uploading…' : 'Change picture' }}
					</button>
					<button
						v-if="user.has_avatar"
						type="button"
						class="text-left text-xs text-ink-subtle hover:text-ink"
						:disabled="avatarBusy"
						@click="dropAvatar"
					>
						Remove
					</button>
					<p v-if="avatarError" class="text-xs text-status-fix-fg">
						{{ avatarError }}
					</p>
				</div>
			</div>

			<TextField
				v-model="user.name"
				:errors="errors.name"
				placeholder="Your name"
			/>

			<div class="flex">
				<TextField
					v-model="user.password"
					:errors="errors?.password"
					class="w-1/2 pr-2"
					placeholder="New password"
				/>

				<TextField
					v-model="user.password_confirmation"
					:errors="errors.password_confirmation"
					class="w-1/2 pl-2"
					placeholder="New password confirmation"
				/>
			</div>

			<div class="text-left">
				<button
					class="mt-4 rounded bg-blue-500 px-8 py-2 font-bold text-white hover:bg-blue-700 focus:outline-none sm:mb-0"
					type="button"
					@click="saveUser"
				>
					Save
				</button>
			</div>
		</div>
	</div>
</template>

<script>
	import {
		forgetAvatar,
		removeAvatar,
		storeAvatar,
	} from '@/actions/tmgr/avatars';
	import { presignUpload, putToStorage } from '@/actions/tmgr/files';
	import { getUser, updateUser } from '@/actions/tmgr/user';
	import Button from '@/components/general/Button.vue';
	import TextField from '@/components/general/TextField.vue';
	import UserAvatar from '@/components/general/UserAvatar.vue';
	import { setDocumentTitle } from '@/composable/useDocumentTitle';

	export default {
		name: 'Profile',
		components: {
			TextField,
			Button,
			UserAvatar,
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
			// Bumped after an upload so the avatar re-reads its link instead of the cached one.
			avatarKey: 0,
		}),
		async mounted() {
			setDocumentTitle('Profile');
			// @todo try to get from store this data first
			this.user = await getUser(); // inside we put response to store. @todo think how to reorganize it or don't care
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
				try {
					const updated = await updateUser(this.user);
					if (updated && typeof updated === 'object')
						this.user = { ...this.user, ...updated };
					this.showAlert('Saved', 'User data saved');
				} catch (error) {
					this.errors = error.response?.data?.errors ?? {};
				}
			},
		},
	};
</script>
