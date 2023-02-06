<template>
	<teleport to="title"> New Password </teleport>

	<AuthBase>
		<template #title>New Password</template>

		<template #body>
			<div
				class="text-center"
				:class="[
					errors && Object.keys(errors).length > 0
						? 'text-red-600'
						: 'font-bold text-neutral-600',
				]"
			>
				{{ message }}
			</div>

			<form
				class="form-horizontal mx-auto flex w-3/4 flex-col gap-2"
				@submit.prevent="resetPassword"
			>
				<TextField
					v-model="form.password"
					:errors="errors.password"
					name="password"
					placeholder="Password"
					input-class="dark:bg-white dark:border-neutral-300"
					type="password"
				/>

				<TextField
					v-model="form.password_confirmation"
					:errors="errors?.password_confirmation"
					name="password_confirmation"
					placeholder="Password confirmation"
					input-class="dark:bg-white dark:border-neutral-300"
					type="password"
				/>

				<div class="mt-2 flex flex-col">
					<button
						class="rounded bg-blue-500 py-2 px-4 text-sm font-semibold text-white hover:bg-blue-700"
						type="submit"
					>
						<span class="relative">
							Reset
							<loader v-if="isLoading" class="auth-loader" is-mini />
						</span>
					</button>
				</div>
			</form>
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
	import { useRouter } from 'vue-router';
	import { ref, onBeforeMount } from 'vue';
	import { setNewPassword } from 'src/actions/tmgr/auth';
	import { AxiosError } from 'axios';
	import AuthBase from 'src/components/layouts/AuthBase.vue';
	import TextField from 'src/components/general/TextField.vue';

	const router = useRouter();

	const isLoading = ref(false);
	const form = ref({
		password: '',
		password_confirmation: '',
	});
	const message = ref('');
	const errors = ref({});
	const token = ref('');

	onBeforeMount(() => {
		const params = new URLSearchParams(location.search);
		token.value = params.get('token') || '';

		if (!token.value) {
			router.push({ name: 'NotFound' });
		}
	});

	async function resetPassword() {
		try {
			message.value = '';
			errors.value = {};
			isLoading.value = true;
			await setNewPassword(token.value, form.value);
			message.value =
				'Your password changed now you can log in with your new password.';
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
