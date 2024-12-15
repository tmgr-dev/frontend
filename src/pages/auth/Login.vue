<template>
	<div class="flex min-h-screen flex-col bg-gray-50">
		<!-- Header Section -->
		<header class="flex items-center justify-between p-4">
			<!-- TM Logo -->
			<div class="flex items-center">
				<img class="h-auto w-12" src="../../assets/img/simple-logo.svg" />
			</div>
		</header>

		<!-- Main Content -->
		<main class="flex flex-1 flex-col md:flex-row">
			<!-- Left Section -->
			<div
				class="hidden w-full flex-col items-center justify-center p-6 md:flex md:w-1/2"
			>
				<!-- Heading -->
				<h1 class="mb-4 text-center text-2xl font-bold md:text-4xl">
					Elevate your efficiency, empower your plans with TMGR
				</h1>
				<!-- Registration Link -->
				<p class="text-center text-sm md:text-lg">
					Have no account?
					<router-link to="/register" class="font-bold text-gray-600"
						>Registration</router-link
					>
				</p>
				<!-- Illustration -->
				<div class="mt-8">
					<img
						src="../../assets/img/login-page.svg"
						alt="Login illustration"
						class="h-auto max-w-full"
					/>
				</div>
			</div>

			<!-- Right Section (Login Form) -->
			<div class="flex w-full items-center justify-center p-4 md:w-1/2 md:p-0">
				<div class="w-full max-w-md rounded-lg bg-white p-8 shadow-lg">
					<!-- Title -->
					<h2 class="mt-6 text-center text-2xl font-bold md:text-3xl">
						Welcome back!
					</h2>
					<div class="mb-8 text-center text-gray-500">Sign in with</div>

					<!-- Social Login Buttons -->
					<div class="mb-8 flex justify-center space-x-4">
						<button
							class="rounded-full bg-gray-200 p-3 hover:bg-gray-300"
							@click="loginWithSocialite('google')"
						>
							<GoogleIcon class="h-6 w-6" />
						</button>
						<button
							class="rounded-full bg-gray-200 p-3 hover:bg-gray-300"
							@click="loginWithSocialite('apple')"
						>
							<AppleIcon class="h-6 w-6" />
						</button>
						<button
							class="rounded-full bg-gray-200 p-3 hover:bg-gray-300"
							@click="loginWithSocialite('github')"
						>
							<GitHubIcon class="h-6 w-6" />
						</button>
					</div>

					<div class="relative mb-8">
						<div class="absolute inset-0 flex items-center">
							<div class="w-full border-t border-gray-300"></div>
						</div>
						<div class="relative flex justify-center text-sm">
							<span class="bg-white px-2 text-gray-500">Or continue with</span>
						</div>
					</div>
					<!-- Login Form -->
					<form @submit.prevent="login">
						<!-- E-mail Field -->
						<div class="mb-4">
							<label for="email" class="block text-sm font-medium"
								>E-mail</label
							>
							<input
								v-model="form.email"
								id="email"
								type="email"
								placeholder="E-mail"
								class="w-full rounded-lg border px-4 py-2"
							/>
						</div>
						<!-- Password Field -->
						<div class="mb-6">
							<label for="password" class="block text-sm font-medium"
								>Password</label
							>
							<div class="relative">
								<input
									v-model="form.password"
									id="password"
									type="password"
									placeholder="Password"
									class="w-full rounded-lg border px-4 py-2"
								/>
							</div>
						</div>
						<!-- Sign In Button -->
						<button
							type="submit"
							class="w-full rounded-lg bg-gray-600 px-4 py-2 text-white hover:bg-gray-700"
						>
							Sign in
						</button>
						<!-- Forgot Password Link -->
						<div class="mt-4 text-center">
							<router-link to="/password/forget" class="text-gray-600"
								>Forgot your password?</router-link
							>
						</div>
						<!-- Login Link -->
						<p class="mt-4 text-center text-sm lg:hidden">
							Have no account?
							<router-link to="/register" class="font-bold text-gray-600"
								>Sign up</router-link
							>
						</p>
					</form>
				</div>
			</div>
		</main>
	</div>
</template>

<script setup lang="ts">
	import { useRouter } from 'vue-router';
	import { ref } from 'vue';
	import { LoginRequest, login as loginAction } from '@/actions/tmgr/auth';
	import { getUser, getUserSettings } from '@/actions/tmgr/user';
	import { AxiosError } from 'axios';
	import { getWorkspaceStatuses } from '@/actions/tmgr/workspaces';
	import store from '@/store';
	import AppleIcon from '@/components/icons/AppleIcon.vue';
	import GitHubIcon from '@/components/icons/GitHubIcon.vue';
	import GoogleIcon from '@/components/icons/GoogleIcon.vue';

	const router = useRouter();

	const form = ref({
		email: '',
		password: '',
	} as LoginRequest);

	const isLoading = ref(false);
	const message = ref('');
	const errors = ref({});

	async function login() {
		try {
			message.value = '';
			errors.value = {};
			isLoading.value = true;
			await loginAction(form.value);
			await getUser();

			if (localStorage.getItem('workspace.invitation')) {
				const token = localStorage.getItem('workspace.invitation');
				await router.push({
					name: 'WorkspaceInvitation',
					params: {
						token,
					},
				});
			} else {
				await router.push({ name: 'CurrentTasksList' });
			}

			if (store.state.user) {
				await Promise.all([getUserSettings(), getWorkspaceStatuses()]);
			}

			store.commit('rerenderApp');
		} catch (error: unknown) {
			if (error instanceof AxiosError) {
				errors.value = error.response?.data?.errors;
				message.value = error.response?.data?.message;
			}
			throw error;
		} finally {
			isLoading.value = false;
		}
	}

	async function loginWithSocialite(platform: string) {
		document.location.href = `${
			import.meta.env.VITE_API_BASE_URL
		}auth/login/${platform}`;
	}
</script>
