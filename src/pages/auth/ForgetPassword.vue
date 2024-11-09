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
					Reset your password and get back to your plans
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

			<!-- Right Section (Reset Password Form) -->
			<div class="flex justify-center items-center w-full md:w-1/2 p-4 md:p-0">
				<div class="w-full max-w-md p-8 bg-white rounded-lg shadow-lg">
					<!-- Title -->
					<h2 class="text-2xl md:text-3xl mt-6 font-bold text-center">Reset Password</h2>
					<div class="text-gray-500 mb-8 text-center">Enter your email to receive reset instructions</div>

					<!-- Success Message -->
					<div v-if="message" class="mb-8">
						<div class="bg-green-50 p-4 rounded-lg">
							<div class="text-green-700 text-center">{{ message }}</div>
						</div>
						<div class="text-center mt-6">
							<router-link to="/login" class="text-gray-600 hover:text-gray-800 font-semibold">
								Back to Login
							</router-link>
						</div>
					</div>

					<!-- Reset Password Form -->
					<form v-else @submit.prevent="sendResetLink">
						<!-- Email Field -->
						<div class="mb-6">
							<label for="email" class="block text-sm font-medium">E-mail</label>
							<input
								v-model="email"
								id="email"
								type="email"
								:class="{'border-red-500': errors.email}"
								placeholder="Enter your email"
								class="w-full px-4 py-2 border rounded-lg"
							/>
							<span v-if="errors.email" class="text-red-500 text-sm mt-1">{{ errors.email[0] }}</span>
						</div>

						<!-- Reset Button -->
						<button
							type="submit"
							:disabled="isLoading"
							class="w-full py-2 px-4 bg-gray-600 text-white rounded-lg hover:bg-gray-700 disabled:opacity-50"
						>
							<span v-if="!isLoading">Send Reset Link</span>
							<span v-else>Sending...</span>
						</button>

						<!-- Login Link -->
						<div class="text-center mt-4">
							<router-link to="/login" class="text-gray-600">Back to Login</router-link>
						</div>

						<!-- Sign Up Link (Mobile) -->
						<div class="mt-6 text-center md:hidden">
							<p class="text-sm text-gray-600">
								Don't have an account?
								<router-link to="/register" class="text-gray-800 font-semibold">Sign up</router-link>
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
import { resetPassword } from 'src/actions/tmgr/auth';

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

		message.value = "The reset link was sent. If you didn't get the email, please check your spam folder.";
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
