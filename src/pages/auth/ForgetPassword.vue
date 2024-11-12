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
					Reset your password and get back to your plans
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

			<!-- Right Section (Reset Password Form) -->
			<div class="flex w-full items-center justify-center p-4 md:w-1/2 md:p-0">
				<div class="w-full max-w-md rounded-lg bg-white p-8 shadow-lg">
					<!-- Title -->
					<h2 class="mt-6 text-center text-2xl font-bold md:text-3xl">
						Reset Password
					</h2>
					<div class="mb-8 text-center text-gray-500">
						Enter your email to receive reset instructions
					</div>

					<!-- Success Message -->
					<div v-if="message" class="mb-8">
						<div class="rounded-lg bg-green-50 p-4">
							<div class="text-center text-green-700">{{ message }}</div>
						</div>
						<div class="mt-6 text-center">
							<router-link
								to="/login"
								class="font-semibold text-gray-600 hover:text-gray-800"
							>
								Back to Login
							</router-link>
						</div>
					</div>

					<!-- Reset Password Form -->
					<form v-else @submit.prevent="sendResetLink">
						<!-- Email Field -->
						<div class="mb-6">
							<label for="email" class="block text-sm font-medium"
								>E-mail</label
							>
							<input
								v-model="email"
								id="email"
								type="email"
								:class="{ 'border-red-500': errors.email }"
								placeholder="Enter your email"
								class="w-full rounded-lg border px-4 py-2"
							/>
							<span v-if="errors.email" class="mt-1 text-sm text-red-500">{{
								errors.email[0]
							}}</span>
						</div>

						<!-- Reset Button -->
						<button
							type="submit"
							:disabled="isLoading"
							class="w-full rounded-lg bg-gray-600 px-4 py-2 text-white hover:bg-gray-700 disabled:opacity-50"
						>
							<span v-if="!isLoading">Send Reset Link</span>
							<span v-else>Sending...</span>
						</button>

						<!-- Login Link -->
						<div class="mt-4 text-center">
							<router-link to="/login" class="text-gray-600"
								>Back to Login</router-link
							>
						</div>

						<!-- Sign Up Link (Mobile) -->
						<div class="mt-6 text-center md:hidden">
							<p class="text-sm text-gray-600">
								Don't have an account?
								<router-link to="/register" class="font-semibold text-gray-800"
									>Sign up</router-link
								>
							</p>
						</div>
					</form>
				</div>
			</div>
		</main>
	</div>
</template>

<script setup lang="ts">
	import { ref } from 'vue';
	import { AxiosError } from 'axios';
	import { resetPassword } from '@/actions/tmgr/auth';

	const email = ref('');
	const message = ref('');
	const isLoading = ref(false);
	const errors = ref({});

	async function sendResetLink() {
		try {
			message.value = '';
			errors.value = {};
			isLoading.value = true;
			await resetPassword({ email: email.value });

			message.value =
				"The reset link was sent. If you didn't get the email, please check your spam folder.";
		} catch (error: unknown) {
			if (error instanceof AxiosError) {
				errors.value = error.response?.data?.errors;
			}
			throw error;
		} finally {
			isLoading.value = false;
		}
	}
</script>
