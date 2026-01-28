<template>
	<div>
		<BaseLayout>
		<template #header>
			<div class="mt-10 w-full text-center">
				<h1 class="mb-4 text-2xl font-bold">Workspace Invitation</h1>
				<p v-if="workspaceName" class="text-lg">
					You have been invited to join: "{{ workspaceName }}"
				</p>
				<p v-else class="text-gray-600">Loading invitation details...</p>
			</div>
		</template>

		<template #body>
			<div class="mx-auto mt-8 max-w-md">
				<!-- Loading State -->
				<div v-if="loading" class="text-center">
					<p class="text-gray-600">Loading...</p>
				</div>

				<!-- Error State -->
				<div v-else-if="error" class="text-center">
					<p class="mb-4 text-red-600">{{ error }}</p>
					<a
						href="#"
						@click="loginWithCorrectEmail()"
						v-if="!user || errorCode === 403"
						class="text-blue-600 hover:underline"
					>
						Login with correct email
					</a>
				</div>

				<!-- Success State -->
				<div v-else-if="accepted" class="text-center">
					<div class="mb-4 rounded-lg bg-green-100 p-4 text-green-800">
						<p class="font-semibold">
							You have successfully joined the workspace!
						</p>
					</div>
					<a
						href="#"
						@click="goToWorkspace()"
						class="text-blue-600 hover:underline"
					>
						Go to workspace
					</a>
				</div>

				<!-- Accept State -->
				<div v-else class="text-center">
					<button
						@click="accept"
						class="w-full rounded bg-blue-600 px-4 py-2 font-bold text-white outline-none transition hover:bg-blue-700 disabled:bg-blue-300"
						type="button"
						:disabled="loading"
					>
						Accept Invitation
					</button>
				</div>
			</div>
		</template>
		</BaseLayout>
	</div>
</template>

<script>
	import { getUser, updateUserSettingsV2 } from '@/actions/tmgr/user';
	import {
		acceptWorkspaceInvitation,
		workspaceInvitationInfo,
	} from '@/actions/tmgr/workspaces';
	import store from '@/store';
	import { setDocumentTitle } from '@/composable/useDocumentTitle';

	export default {
		name: 'WorkspaceInvitation',

		data: () => ({
			loading: true,
			accepted: false,
			error: null,
			errorCode: null,
			workspaceName: '',
			workspaceId: null,
			user: null,
		}),

		computed: {
			workspaceInvitationToken() {
				localStorage.setItem('workspace.invitation', this.$route.params.token);
				return this.$route.params.token;
			},
		},

		async mounted() {
			setDocumentTitle('Workspace Invitation');
			try {
				this.user = await getUser();
				const data = await workspaceInvitationInfo(
					this.workspaceInvitationToken,
				);
				this.workspaceName = data.workspace.name;
				this.workspaceId = data.workspace.id;
				this.loading = false;
			} catch (e) {
				console.log(e);
				this.error =
					e.response?.data?.message || 'Failed to load invitation details';
				this.errorCode = e.response?.status;
				this.loading = false;
			}
		},

		methods: {
			loginWithCorrectEmail() {
				this.$store.dispatch('logout');
			},
			async goToWorkspace() {
				const settingsWithUpdatedWorkspace = this.user.settings.map(
					(setting) => {
						if (setting.key === 'current_workspace') {
							setting.value = this.workspaceId;
						}
						return {
							id: setting.id,
							value: setting.value,
						};
					},
				);

				await updateUserSettingsV2(settingsWithUpdatedWorkspace);

				setTimeout(() => {
					this.$store.commit('rerenderApp');
					this.$router.push({ name: 'CurrentTasksList' });
				}, 1000);
			},
			async accept() {
				this.loading = true;
				this.error = null;

				try {
					await acceptWorkspaceInvitation(this.workspaceInvitationToken);
					this.accepted = true;
					localStorage.removeItem('workspace.invitation');
				} catch (e) {
					this.error =
						e.response?.data?.message || 'Failed to accept invitation';
				} finally {
					this.loading = false;
				}
			},
		},
	};
</script>
