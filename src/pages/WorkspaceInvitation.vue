<template>
	<div>
		<BaseLayout
			width="narrow"
			:title="$t('workspace.workspaceInvitation.title')"
			:subtitle="invitationSubtitle"
		>
			<template #body>
				<div>
					<!-- Loading State -->
					<div v-if="loading">
						<p class="text-ink-muted">
							{{ $t('workspace.workspaceInvitation.loading') }}
						</p>
					</div>

					<!-- Guest State -->
					<div v-else-if="isGuest" class="flex flex-col gap-3">
						<Button type="button" @click="goTo('Register')">
							{{ $t('auth.register.submit') }}
						</Button>
						<Button type="button" variant="outline" @click="goTo('Login')">
							{{ $t('auth.common.signIn') }}
						</Button>
					</div>

					<!-- Error State -->
					<div v-else-if="error">
						<p class="mb-4 text-status-fix-fg">{{ error }}</p>
						<a
							href="#"
							v-if="needsVerification"
							@click.prevent="load()"
							class="text-brand hover:underline"
						>
							{{ $t('auth.common.tryAgain') }}
						</a>
						<a
							href="#"
							@click="loginWithCorrectEmail()"
							v-else-if="!user || errorCode === 403"
							class="text-brand hover:underline"
						>
							{{ $t('workspace.workspaceInvitation.loginCorrect') }}
						</a>
					</div>

					<!-- Success State -->
					<div v-else-if="accepted">
						<div
							class="mb-4 rounded-lg bg-status-done-bg p-4 text-status-done-fg"
						>
							<p class="font-semibold">
								{{ $t('workspace.workspaceInvitation.joined') }}
							</p>
						</div>
						<a
							href="#"
							@click="goToWorkspace()"
							class="text-brand hover:underline"
						>
							{{ $t('workspace.workspaceInvitation.goTo') }}
						</a>
					</div>

					<!-- Accept State -->
					<div v-else>
						<Button @click="accept" type="button" :disabled="loading">
							{{ $t('workspace.workspaceInvitation.accept') }}
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
	import { t } from '@/i18n';
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
			isGuest: false,
		}),

		computed: {
			workspaceInvitationToken() {
				return this.$route.params.token;
			},
			needsVerification() {
				return this.errorCode === 403 && /verify your email/i.test(this.error);
			},
			invitationSubtitle() {
				if (this.isGuest) return t('workspace.workspaceInvitation.guestHint');
				return this.workspaceName
					? t('workspace.workspaceInvitation.invitedTo', {
							name: this.workspaceName,
						})
					: '';
			},
		},

		async mounted() {
			setDocumentTitle(t('workspace.workspaceInvitation.title'));
			localStorage.setItem(
				'workspace.invitation',
				this.workspaceInvitationToken,
			);
			if (!this.$store.getters.isLoggedIn) {
				this.isGuest = true;
				this.loading = false;
				return;
			}
			await this.load();
		},

		methods: {
			async load() {
				this.loading = true;
				this.error = null;
				this.errorCode = null;
				try {
					this.user = await getUser();
					const data = await workspaceInvitationInfo(
						this.workspaceInvitationToken,
					);
					this.workspaceName = data.workspace.name;
					this.workspaceId = data.workspace.id;
					if (data.already_member) {
						localStorage.removeItem('workspace.invitation');
						this.accepted = true;
					}
					this.loading = false;
				} catch (e) {
					console.log(e);
					this.error =
						e.response?.data?.message ||
						t('workspace.workspaceInvitation.loadFailed');
					this.errorCode = e.response?.status;
					this.forgetSettledInvitation(e);
					this.loading = false;
				}
			},
			forgetSettledInvitation(e) {
				const message = e.response?.data?.message ?? '';
				if (
					[404, 422].includes(e.response?.status) ||
					/already/i.test(message)
				) {
					localStorage.removeItem('workspace.invitation');
				}
			},
			goTo(name) {
				this.$router.push({ name });
			},
			async loginWithCorrectEmail() {
				await this.$store.dispatch('logout');
				this.$router.push({ name: 'Login' });
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
						e.response?.data?.message ||
						t('workspace.workspaceInvitation.acceptFailed');
					this.forgetSettledInvitation(e);
				} finally {
					this.loading = false;
				}
			},
		},
	};
</script>
