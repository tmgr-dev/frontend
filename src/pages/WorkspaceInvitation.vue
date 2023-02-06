<template>
	<teleport to="title">Invitation to the workspace</teleport>

	<BaseLayout>
		<template #header>
			<span class="mt-10 block w-full text-center">
				You have been invited to the workspace:
			</span>
		</template>

		<template #body>
			<div class="mx-auto max-w-sm" v-if="!accepted">
				<button
					@click="accept"
					class="w-full rounded bg-orange-500 py-2 px-4 font-bold text-white outline-none transition hover:bg-orange-600"
					type="button"
				>
					Accept
				</button>
			</div>

			<div
				class="mt-6 text-center text-xl font-bold text-blue-800 dark:text-white"
			>
				<span v-if="accepted"> You successfully accept invitation! </span>
				<span v-if="message" class="text-red-600">{{ message }}</span>
			</div>
		</template>
	</BaseLayout>
</template>

<script>
	import Button from 'src/components/UIElements/Button.vue';
	import { getUser, updateUser } from 'src/actions/tmgr/user';
	import { acceptWorkspaceInvitation } from 'src/actions/tmgr/workspaces';

	export default {
		name: 'WorkspaceInvitation',
		components: { Button },
		data: () => ({
			accepted: false,
			workspace: {
				name: 'Workspace name',
			},
			user: {
				name: null,
				password: null,
				password_confirmation: null,
			},
			message: '',
			errors: {},
		}),
		computed: {
			workspaceInvitationToken() {
				return this.$route.params.token;
			},
		},
		async mounted() {
			this.user = await getUser();
		},
		methods: {
			async accept() {
				try {
					await acceptWorkspaceInvitation(this.workspaceInvitationToken);
					this.accepted = true;
				} catch (e) {
					this.message = e.response?.data?.message;
				}
			},
			async saveUser() {
				try {
					this.user = await updateUser(this.user);
					this.showAlert('Saved', 'User data saved');
				} catch (e) {
					this.errors = e.response?.data?.errors;
				}
			},
		},
	};
</script>
