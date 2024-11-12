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
					Join TMGR and start organizing your tasks
				</h1>
				<!-- Login Link -->
				<p class="text-center text-sm md:text-lg">
					Already have an account?
					<router-link to="/login" class="font-bold text-gray-600"
						>Sign in</router-link
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

			<!-- Right Section (Registration Form) -->
			<div class="flex w-full items-center justify-center p-4 md:w-1/2 md:p-0">
				<div class="w-full max-w-md rounded-lg bg-white p-8 shadow-lg">
					<!-- Title -->
					<h2 class="mt-6 text-center text-2xl font-bold md:text-3xl">
						Create Account
					</h2>
					<div class="mb-8 text-center text-gray-500">Sign up with</div>

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

					<!-- Registration Form -->
					<form @submit.prevent="register">
						<!-- Name Field -->
						<div class="mb-4">
							<label for="name" class="block text-sm font-medium">Name</label>
							<input
								v-model="form.name"
								id="name"
								type="text"
								:class="{ 'border-red-500': errors.name }"
								placeholder="Your name"
								class="w-full rounded-lg border px-4 py-2"
							/>
							<span v-if="errors.name" class="text-sm text-red-500">{{
								errors.name[0]
							}}</span>
						</div>

						<!-- Email Field -->
						<div class="mb-4">
							<label for="email" class="block text-sm font-medium"
								>E-mail</label
							>
							<input
								v-model="form.email"
								id="email"
								type="email"
								:class="{ 'border-red-500': errors.email }"
								placeholder="your@email.com"
								class="w-full rounded-lg border px-4 py-2"
							/>
							<span v-if="errors.email" class="text-sm text-red-500">{{
								errors.email[0]
							}}</span>
						</div>

						<!-- Password Field -->
						<div class="mb-4">
							<label for="password" class="block text-sm font-medium"
								>Password</label
							>
							<input
								v-model="form.password"
								id="password"
								type="password"
								:class="{ 'border-red-500': errors.password }"
								placeholder="Create a password"
								class="w-full rounded-lg border px-4 py-2"
							/>
							<span v-if="errors.password" class="text-sm text-red-500">{{
								errors.password[0]
							}}</span>
						</div>

						<!-- Password Confirmation Field -->
						<div class="mb-6">
							<label
								for="password_confirmation"
								class="block text-sm font-medium"
								>Confirm Password</label
							>
							<input
								v-model="form.password_confirmation"
								id="password_confirmation"
								type="password"
								placeholder="Confirm your password"
								class="w-full rounded-lg border px-4 py-2"
							/>
						</div>

						<!-- Register Button -->
						<button
							type="submit"
							:disabled="isLoading"
							class="w-full rounded-lg bg-gray-600 px-4 py-2 text-white hover:bg-gray-700 disabled:opacity-50"
						>
							<span v-if="!isLoading">Create Account</span>
							<span v-else>Creating...</span>
						</button>
					</form>

					<!-- Login Link (Mobile) -->
					<div class="mt-6 text-center md:hidden">
						<p class="text-sm text-gray-600">
							Already have an account?
							<router-link to="/login" class="font-semibold text-gray-800"
								>Sign in</router-link
							>
						</p>
					</div>
				</div>
			</div>
		</main>
	</div>
</template>

<script setup lang="ts">
	import { useRouter } from 'vue-router';
	import { ref } from 'vue';
	import { Register, register as registerAction } from '@/actions/tmgr/auth';
	import { AxiosError } from 'axios';
	import { getUser } from '@/actions/tmgr/user';
	import AppleIcon from '@/components/icons/AppleIcon.vue';
	import GitHubIcon from '@/components/icons/GitHubIcon.vue';
	import GoogleIcon from '@/components/icons/GoogleIcon.vue';

	const router = useRouter();

	const isLoading = ref(false);
	const errors = ref({});
	const form = ref({
		name: '',
		email: '',
		password: '',
		password_confirmation: '',
	} as Register);

	async function register() {
		try {
			errors.value = {};
			isLoading.value = true;
			await registerAction(form.value);
			await getUser(true);

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
		} catch (error: unknown) {
			if (error instanceof AxiosError) {
				errors.value = error.response?.data?.errors;
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
