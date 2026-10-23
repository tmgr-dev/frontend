<template>
	<div>
		<BaseLayout
			width="narrow"
			title="Workspace Invitation"
			:subtitle="invitationSubtitle"
		>
			<template #body>
				<div>
					<!-- Loading State -->
					<div v-if="loading">
						<p class="text-ink-muted">Loading invitation details...</p>
					</div>

					<!-- Error State -->
					<div v-else-if="error">
						<p class="mb-4 text-status-fix-fg">{{ error }}</p>
						<a
							href="#"
							@click="loginWithCorrectEmail()"
							v-if="!user || errorCode === 403"
							class="text-brand hover:underline"
						>
							Login with correct email
						</a>
					</div>

					<!-- Success State -->
					<div v-else-if="accepted">
						<div
							class="mb-4 rounded-lg bg-status-done-bg p-4 text-status-done-fg"
						>
							<p class="font-semibold">
								You have successfully joined the workspace!
							</p>
						</div>
						<a
							href="#"
							@click="goToWorkspace()"
							class="text-brand hover:underline"
						>
							Go to workspace
						</a>
					</div>

					<!-- Accept State -->
					<div v-else>
						<Button @click="accept" type="button" :disabled="loading">
							Accept Invitation
						</Button>
					</div>
				</div>
			</template>
		</BaseLayout>
	</div>
</template>

<script>
	import { getUser } from '@/actions/tmgr/user';
	import {
		acceptWorkspaceInvitation,
		getWorkspaces,
		workspaceInvitationInfo,
	} from '@/actions/tmgr/workspaces';
	import { Button } from '@/components/ui/button';
	import { setDocumentTitle } from '@/composable/useDocumentTitle';
	import { requestCache } from '@/utils/requestCache';

	export default {
		name: 'WorkspaceInvitation',

		components: { Button },

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
			invitationSubtitle() {
				return this.workspaceName
					? `You have been invited to join: "${this.workspaceName}"`
					: '';
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
				requestCache.invalidate('workspaces');
				const workspaces = await getWorkspaces();
				this.$store.commit('setWorkspaces', workspaces);
				this.$store.commit('updateUserWorkspaceSetting', {
					workspaceId: this.workspaceId,
				});

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
