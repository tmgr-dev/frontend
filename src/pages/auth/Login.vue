<template>
	<div class="flex flex-col md:flex-row h-screen">
		<!-- Left Section -->
		<div class="hidden md:flex flex-col justify-center items-center w-full md:w-1/2 bg-white p-6">
			<!-- TMGR Logo -->
			<img src="path_to_logo" alt="Logo" class="w-24 h-auto mb-6">
			<!-- Heading -->
			<h1 class="text-2xl md:text-4xl font-bold mb-4 text-center">
				Elevate your efficiency, empower your plans with TMGR
			</h1>
			<!-- Registration Link -->
			<p class="text-sm md:text-lg text-center">
				Have no account?
				<router-link to="/register" class="text-blue-600">Registration</router-link>
			</p>
			<!-- Illustration -->
			<div class="mt-8">
				<img src="path_to_your_illustration" alt="Illustration" class="w-48 md:w-80 h-auto">
			</div>
		</div>

		<!-- Right Section (Login Form) -->
		<div class="flex justify-center items-center w-full md:w-1/2 bg-gray-50 p-4 md:p-0">
			<div class="w-full max-w-md p-8 bg-white rounded-lg shadow-lg">
				<!-- Title -->
				<h2 class="text-2xl md:text-3xl font-bold mb-6 text-center">Welcome back!</h2>
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
					<div class="mb-4">
						<label for="password" class="block text-sm font-medium">Password</label>
						<div class="relative">
							<input
								v-model="form.password"
								id="password"
								type="password"
								placeholder="Password"
								class="w-full px-4 py-2 border rounded-lg"
							/>
							<span class="absolute inset-y-0 right-4 flex items-center">✈️</span>
						</div>
					</div>
					<!-- Sign In Button -->
					<button
						type="submit"
						class="w-full py-2 px-4 bg-blue-600 text-white rounded-lg hover:bg-blue-700"
					>
						Sign in
					</button>
					<!-- Forgot Password Link -->
					<div class="text-center mt-4">
						<router-link to="/password/forget" class="text-blue-600">Forgot your password?</router-link>
					</div>
					<!-- Social Media Login Icons -->
					<div class="flex justify-center mt-6 space-x-4">
						<button class="bg-gray-200 p-3 rounded-full">
							<img src="google_icon_path" alt="Google" class="w-5 h-5">
						</button>
						<button class="bg-gray-200 p-3 rounded-full">
							<img src="apple_icon_path" alt="Apple" class="w-5 h-5">
						</button>
						<button class="bg-gray-200 p-3 rounded-full">
							<img src="github_icon_path" alt="GitHub" class="w-5 h-5">
						</button>
					</div>
				</form>
			</div>
		</div>
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
			await router.push({ name: 'CurrentTasksList' });

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
</script>
