<template>
	<div>
		<AuthBase>
			<template #title>Welcome back!</template>
			<template #body>
				<div class="text-center" role="status" :aria-busy="isLoading">
					{{ message }}
					<router-link
						v-if="!isLoading"
						to="/login"
						class="mt-3 block underline"
						>Return to sign in</router-link
					>
				</div>
			</template>
		</AuthBase>
	</div>
</template>

<script setup lang="ts">
	import {
		loginApple,
		loginGithub,
		loginGoogle,
		LoginGoogleRequest,
		LoginRequest,
		loginTelegram,
		LoginWithCodeRequest,
		setTokenAndHeaders,
	} from '@/actions/tmgr/auth';
	import { getUser, getUserSettings } from '@/actions/tmgr/user';
	import { getWorkspaceStatuses } from '@/actions/tmgr/workspaces';
	import AuthBase from '@/components/layouts/AuthBase.vue';
	import { setDocumentTitle } from '@/composable/useDocumentTitle';
	import store from '@/store';
	import { AxiosError } from 'axios';
	import { ref } from 'vue';
	import { useRoute, useRouter } from 'vue-router';

	const router = useRouter();
	const route = useRoute();

	const form = ref({
		email: '',
		password: '',
	} as LoginRequest);

	// @todo for savayer only. Find out how to make it global
	const isLoading = ref(false);
	const message = ref('Wait for redirect...');
	const errors = ref({});
	async function login() {
		// Check for token from Telegram Widget redirect
		if (route.query.token) {
			try {
				isLoading.value = true;
				const token = route.query.token as string;
				setTokenAndHeaders(token);

				// Fetch user data and redirect
				await getUser();
				if (store.state.user) {
					await Promise.all([
						getUserSettings(),
						getWorkspaceStatuses(),
						store.dispatch('featureToggles/loadUserToggles'),
					]);
				}

				const landingPage =
					store.getters['featureToggles/getUserFeatureValue'](
						'default_landing_page',
					) || 'list';
				const currentWorkspaceId = store.state.user?.settings?.find(
					(s) => s.key === 'current_workspace',
				)?.value;
				const workspace = store.state.workspaces?.find(
					(w) => w.id == currentWorkspaceId,
				);
				const workspaceCode = workspace?.code;

				if (workspaceCode) {
					await router.push(`/${workspaceCode}/${landingPage}`);
				} else {
					await router.push({ name: 'CurrentTasksList' });
				}

				store.commit('rerenderApp');
				return; // Exit early
			} catch (error: unknown) {
				message.value = 'Failed to process Telegram login. Redirecting...';
				console.error('Telegram token processing error:', error);
				setTimeout(() => router.push('/login'), 3000);
				// The status above reports the error; OAuth codes must not be retried.
			} finally {
				isLoading.value = false;
			}
		}

		const platform = route.params.platform;
		try {
			message.value = '';
			errors.value = {};
			isLoading.value = true;
			switch (platform) {
				case 'github':
					const githubPayload: LoginWithCodeRequest = {
						code: route.query?.code as string,
					};
					await loginGithub(githubPayload);
					break;
				case 'apple':
					const applePayload: LoginWithCodeRequest = {
						code: route.query?.code as string,
					};
					await loginApple(applePayload);
					break;
				case 'google':
					const googlePayload: LoginGoogleRequest = {
						code: route.query?.code as string,
						scope: route.query?.scope as string,
						ail: route.query?.ail as string,
						authuser: parseInt(route.query?.authuser as string),
						prompt: route.query?.prompt as string,
					};
					await loginGoogle(googlePayload);
					break;
				case 'telegram':
					const telegramPayload: LoginWithCodeRequest = {
						code: route.query?.code as string,
					};
					await loginTelegram(telegramPayload);
					break;
				default:
					await router.push('login');
			}
			await getUser();

			if (store.state.user) {
				await Promise.all([
					getUserSettings(),
					getWorkspaceStatuses(),
					store.dispatch('featureToggles/loadUserToggles'),
				]);
			}

			const landingPage =
				store.getters['featureToggles/getUserFeatureValue'](
					'default_landing_page',
				) || 'list';
			const currentWorkspaceId = store.state.user?.settings?.find(
				(s) => s.key === 'current_workspace',
			)?.value;
			const workspace = store.state.workspaces?.find(
				(w) => w.id == currentWorkspaceId,
			);
			const workspaceCode = workspace?.code;

			if (workspaceCode) {
				await router.push(`/${workspaceCode}/${landingPage}`);
			} else {
				await router.push({ name: 'CurrentTasksList' });
			}

			store.commit('rerenderApp');
		} catch (error: unknown) {
			if (error instanceof AxiosError) {
				errors.value = error.response?.data?.errors;

				message.value = `Something went wrong: ${error.response?.data?.message}... After 3 seconds you will redirected to the login page.`;
				setTimeout(() => router.push('/login'), 3000);
			}

			// The status above reports the error; OAuth codes must not be retried.
		} finally {
			isLoading.value = false;
		}
	}

	setDocumentTitle('Login');
	login();
</script>
