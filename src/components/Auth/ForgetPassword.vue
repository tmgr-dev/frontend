<template>
	<teleport to="title"> Restore Password </teleport>

	<AuthBase>
		<template #title>Restore Password</template>

		<template #body>
			<form
				v-if="!message"
				action="#"
				class="form-horizontal mx-auto w-3/4"
				method="POST"
			>
				<div class="mt-4 flex flex-col">
					<TextField
						v-model="email"
						:errors="errors.email"
						name="email"
						placeholder="Email"
						type="email"
					/>
				</div>

				<div class="mt-6 flex flex-col">
					<button
						class="rounded bg-blue-500 py-2 px-4 text-center text-sm font-semibold text-white hover:bg-blue-700"
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
				class="text-blue-dark text-xs no-underline hover:underline"
				to="/register"
			>
				You don't have account?
			</router-link>
			<br />
			<router-link
				class="text-blue-dark text-xs no-underline hover:underline"
				to="/login"
			>
				Login
			</router-link>
		</template>
	</AuthBase>
</template>

<script setup lang="ts">
	import AuthBase from 'src/components/Layouts/AuthBase.vue';
	import Button from 'src/components/UIElements/Button';
	import { ref } from 'vue';
	import { AxiosError } from 'axios';
	import { resetPassword } from 'src/actions/tmgr/auth';
	import TextField from 'src/components/general/TextField.vue';

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
