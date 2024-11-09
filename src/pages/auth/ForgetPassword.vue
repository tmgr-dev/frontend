<template>
	<div class="min-h-screen flex flex-col bg-gray-50">
		<!-- Header Section -->
		<header class="flex justify-between items-center p-4">
			<!-- TM Logo -->
			<div class="flex items-center">
				<img class="w-12 h-auto" src="../../assets/img/simple-logo.svg" alt="TM Logo">
			</div>
		</header>

		<!-- Main Content -->
		<main class="flex-1 flex flex-col md:flex-row">
			<!-- Left Section -->
			<div class="hidden md:flex flex-col justify-center items-center w-full md:w-1/2 p-6">
				<!-- Heading -->
				<h1 class="text-2xl md:text-4xl font-bold mb-4 text-center">
					Join TMGR and start organizing your tasks
				</h1>
				<!-- Login Link -->
				<p class="text-sm md:text-lg text-center">
					Already have an account?
					<router-link to="/login" class="text-gray-600 font-bold">Sign in</router-link>
				</p>
				<!-- Illustration -->
				<div class="mt-8">
					<img src="../../assets/img/login-page.svg" alt="Login illustration" class="max-w-full h-auto">
				</div>
			</div>

			<!-- Right Section (Registration Form) -->
			<div class="flex justify-center items-center w-full md:w-1/2 p-4 md:p-0">
				<div class="w-full max-w-md p-8 bg-white rounded-lg shadow-lg">
					<!-- Title -->
					<h2 class="text-2xl md:text-3xl mt-6 font-bold text-center">Create Account</h2>
					<div class="text-gray-500 mb-8 text-center">Sign up with</div>

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

					<!-- Registration Form -->
					<form @submit.prevent="register">
						<!-- Name Field -->
						<div class="mb-4">
							<label for="name" class="block text-sm font-medium">Name</label>
							<input
								v-model="form.name"
								id="name"
								type="text"
								:class="{'border-red-500': errors.name}"
								placeholder="Your name"
								class="w-full px-4 py-2 border rounded-lg"
							/>
							<span v-if="errors.name" class="text-red-500 text-sm">{{ errors.name[0] }}</span>
						</div>

						<!-- Email Field -->
						<div class="mb-4">
							<label for="email" class="block text-sm font-medium">E-mail</label>
							<input
								v-model="form.email"
								id="email"
								type="email"
								:class="{'border-red-500': errors.email}"
								placeholder="your@email.com"
								class="w-full px-4 py-2 border rounded-lg"
							/>
							<span v-if="errors.email" class="text-red-500 text-sm">{{ errors.email[0] }}</span>
						</div>

						<!-- Password Field -->
						<div class="mb-4">
							<label for="password" class="block text-sm font-medium">Password</label>
							<input
								v-model="form.password"
								id="password"
								type="password"
								:class="{'border-red-500': errors.password}"
								placeholder="Create a password"
								class="w-full px-4 py-2 border rounded-lg"
							/>
							<span v-if="errors.password" class="text-red-500 text-sm">{{ errors.password[0] }}</span>
						</div>

						<!-- Password Confirmation Field -->
						<div class="mb-6">
							<label for="password_confirmation" class="block text-sm font-medium">Confirm Password</label>
							<input
								v-model="form.password_confirmation"
								id="password_confirmation"
								type="password"
								placeholder="Confirm your password"
								class="w-full px-4 py-2 border rounded-lg"
							/>
						</div>

						<!-- Register Button -->
						<button
							type="submit"
							:disabled="isLoading"
							class="w-full py-2 px-4 bg-gray-600 text-white rounded-lg hover:bg-gray-700 disabled:opacity-50"
						>
							<span v-if="!isLoading">Create Account</span>
							<span v-else>Creating...</span>
						</button>
					</form>

					<!-- Login Link (Mobile) -->
					<div class="mt-6 text-center md:hidden">
						<p class="text-sm text-gray-600">
							Already have an account?
							<router-link to="/login" class="text-gray-800 font-semibold">Sign in</router-link>
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
import { Register, register as registerAction } from 'src/actions/tmgr/auth';
import { AxiosError } from 'axios';
import { getUser } from 'src/actions/tmgr/user';
import AppleIcon from 'src/components/icons/AppleIcon.vue';
import GitHubIcon from 'src/components/icons/GitHubIcon.vue';
import GoogleIcon from 'src/components/icons/GoogleIcon.vue';

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
					token
				}
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
	document.location.href = `${process.env.VUE_APP_API_BASE_URL}auth/login/${platform}`;
}
</script>
