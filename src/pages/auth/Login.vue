<template>
	<div class="min-h-screen flex flex-col bg-gray-50">
		<!-- Header Section -->
		<header class="flex justify-between items-center p-4">
			<!-- TM Logo -->
			<div class="flex items-center">
				<img class="w-12 h-auto" src="../../assets/img/simple-logo.svg">
			</div>
		</header>

		<!-- Main Content -->
		<main class="flex-1 flex flex-col md:flex-row">
			<!-- Left Section -->
			<div class="hidden md:flex flex-col justify-center items-center w-full md:w-1/2 p-6">
				<!-- Heading -->
				<h1 class="text-2xl md:text-4xl font-bold mb-4 text-center">
					Elevate your efficiency, empower your plans with TMGR
				</h1>
				<!-- Registration Link -->
				<p class="text-sm md:text-lg text-center">
					Have no account?
					<router-link to="/register" class="text-gray-600 font-bold">Registration</router-link>
				</p>
				<!-- Illustration -->
				<div class="mt-8">
					<img src="../../assets/img/login-page.svg" alt="Login illustration" class="max-w-full h-auto">
				</div>
			</div>

			<!-- Right Section (Login Form) -->
			<div class="flex justify-center items-center w-full md:w-1/2 p-4 md:p-0">
				<div class="w-full max-w-md p-8 bg-white rounded-lg shadow-lg">
					<!-- Title -->
					<h2 class="text-2xl md:text-3xl mt-6 font-bold text-center">Welcome back!</h2>
					<div class="text-gray-500 mb-8 text-center">Sign in with</div>

					<!-- Social Login Buttons -->
					<div class="flex justify-center space-x-4 mb-8">
						<button class="bg-gray-200 p-3 rounded-full hover:bg-gray-300"
										@click="loginWithSocialite('google')"
						>
							<GoogleIcon class="w-6 h-6"/>
						</button>
						<button class="bg-gray-200 p-3 rounded-full hover:bg-gray-300"
										@click="loginWithSocialite('apple')"
						>
							<AppleIcon class="w-6 h-6"/>
						</button>
						<button class="bg-gray-200 p-3 rounded-full hover:bg-gray-300"
										@click="loginWithSocialite('github')"
						>
							<GitHubIcon class="w-6 h-6"/>
						</button>
					</div>

					<div class="relative mb-8">
						<div class="absolute inset-0 flex items-center">
							<div class="w-full border-t border-gray-300"></div>
						</div>
						<div class="relative flex justify-center text-sm">
							<span class="px-2 bg-white text-gray-500">Or continue with</span>
						</div>
					</div>
					<!-- Login Form -->
					<form @submit.prevent="login">
						<!-- E-mail Field -->
						<div class="mb-4">
							<label for="email" class="block text-sm font-medium">E-mail</label>
							<input
								v-model="form.email"
								id="email"
								type="email"
								placeholder="E-mail"
								class="w-full px-4 py-2 border rounded-lg"
							/>
						</div>
						<!-- Password Field -->
						<div class="mb-6">
							<label for="password" class="block text-sm font-medium">Password</label>
							<div class="relative">
								<input
									v-model="form.password"
									id="password"
									type="password"
									placeholder="Password"
									class="w-full px-4 py-2 border rounded-lg"
								/>
							</div>
						</div>
						<!-- Sign In Button -->
						<button
							type="submit"
							class="w-full py-2 px-4 bg-gray-600 text-white rounded-lg hover:bg-gray-700"
						>
							Sign in
						</button>
						<!-- Forgot Password Link -->
						<div class="text-center mt-4">
							<router-link to="/password/forget" class="text-gray-600">Forgot your password?</router-link>
						</div>
						<!-- Login Link -->
						<p class="lg:hidden text-sm text-center mt-4">
							Have no account?
							<router-link to="/register" class="text-gray-600 font-bold">Sign up</router-link>
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
	import { LoginRequest, login as loginAction } from 'src/actions/tmgr/auth';
	import { getUser, getUserSettings } from 'src/actions/tmgr/user';
	import { AxiosError } from 'axios';
	import { getWorkspaceStatuses } from 'src/actions/tmgr/workspaces';
	import store from 'src/store';
	import AppleIcon from 'src/components/icons/AppleIcon.vue';
	import GitHubIcon from 'src/components/icons/GitHubIcon.vue';
	import GoogleIcon from 'src/components/icons/GoogleIcon.vue';

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
			await getUser(true);

			if (localStorage.getItem('workspace.invitation')) {
				const token = localStorage.getItem('workspace.invitation');
				await router.push({
					name: 'WorkspaceInvitation',
					params: {
						token
					}
				});
			} else {
				await router.push({ name: 'CurrentTasksList' });
			}

			if (store.state.user) {
				await Promise.all([getUserSettings(), getWorkspaceStatuses()]);
			}
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
		document.location.href = `${process.env.VUE_APP_API_BASE_URL}auth/login/${platform}`;
	}
</script>
