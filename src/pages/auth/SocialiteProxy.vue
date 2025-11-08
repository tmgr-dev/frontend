<template>
	<teleport to="title">Login</teleport>

	<AuthBase>
		<template #title>Welcome back!</template>
		<template #body>
			<div class="text-center">
				{{ message }}
			</div>
		</template>
	</AuthBase>
</template>

<script setup lang="ts">
	import { useRouter, useRoute } from 'vue-router';
	import { ref } from 'vue';
	import store from '@/store';
	import {
		LoginRequest,
		loginGoogle,
		loginApple,
		loginGithub,
		LoginWithCodeRequest,
		LoginGoogleRequest,
		loginTelegram,
		setTokenAndHeaders,
	} from '@/actions/tmgr/auth';
	import { getUser, getUserSettings } from '@/actions/tmgr/user';
	import { AxiosError } from 'axios';
	import AuthBase from '@/components/layouts/AuthBase.vue';
	import { getWorkspaceStatuses } from '@/actions/tmgr/workspaces';

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
					await Promise.all([getUserSettings(), getWorkspaceStatuses()]);
				}
				await router.push({ name: 'CurrentTasksList' });
				store.commit('rerenderApp'); // Assuming this is needed after login
				return; // Exit early
			} catch (error: unknown) {
				message.value = 'Failed to process Telegram login. Redirecting...';
				console.error('Telegram token processing error:', error);
				setTimeout(() => router.push('/login'), 3000);
				throw error;
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
			await router.push({ name: 'CurrentTasksList' });

			if (store.state.user) {
				await Promise.all([getUserSettings(), getWorkspaceStatuses()]);
			}
		} catch (error: unknown) {
			if (error instanceof AxiosError) {
				errors.value = error.response?.data?.errors;

				message.value = `Something went wrong: ${error.response?.data?.message}... After 3 seconds you will redirected to the login page.`;
				setTimeout(() => router.push('/login'), 3000);
			}

			throw error;
		} finally {
			isLoading.value = false;
		}
	}

	login();
</script>
