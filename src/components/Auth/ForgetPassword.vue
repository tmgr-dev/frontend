<template>
	<teleport to="title"> Restore Password </teleport>

	<AuthBase>
		<template #title>Restore Password</template>

		<template #body>
			<form
				v-if="!message"
				action="#"
				class="form-horizontal w-3/4 mx-auto"
				method="POST"
			>
				<div class="flex flex-col mt-4">
					<input-field
						id="email"
						v-model="email"
						:errors="errors?.email"
						name="email"
						placeholder="Email"
						type="text"
					/>
				</div>

				<div class="flex flex-col mt-6">
					<button
						class="bg-blue-500 text-center hover:bg-blue-700 text-white text-sm font-semibold py-2 px-4 rounded"
						@click.prevent="sendResetLink"
					>
						<span class="relative">
							Send link
							<loader v-if="isLoading" class="auth-loader" is-mini />
						</span>
					</button>
				</div>
			</form>

			<p v-else class="text-center">
				{{ message }}
			</p>
		</template>

		<template #footer>
			<router-link
				class="no-underline hover:underline text-blue-dark text-xs"
				to="/register"
			>
				You don't have account?
			</router-link>
			<br />
			<router-link
				class="no-underline hover:underline text-blue-dark text-xs"
				to="/login"
			>
				Login
			</router-link>
		</template>
	</AuthBase>
</template>

<script setup lang="ts">
	import AuthBase from 'src/components/Auth/AuthBase';
	import Button from 'src/components/UIElements/Button';
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

			message.value =
				"The reset link was sent. If you didn't get the email, please try check out your spam.";
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
