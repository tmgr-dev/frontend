<template>
	<teleport to="title"> Login </teleport>

	<AuthBase>
		<template #title>Welcome back!</template>

		<template #body>
			<form class="form-horizontal w-3/4 mx-auto" @submit.prevent="login">
				<div class="flex flex-col mt-4">
					<div :style="{ color: errors ? 'red' : 'initial' }">
						{{ message }}
					</div>
				</div>

				<div class="flex flex-col mt-4">
					<input-field
						v-model="form.email"
						:errors="errors.email"
						name="email"
						placeholder="Email"
						type="email"
					/>
				</div>

				<div class="flex flex-col mt-4">
					<input-field
						v-model="form.password"
						:errors="errors.password"
						name="password"
						placeholder="Password"
						type="password"
					/>
				</div>

				<div class="flex flex-col mt-6">
					<button
						class="bg-blue-500 hover:bg-blue-700 text-white text-sm font-semibold py-2 px-4 rounded"
						type="submit"
					>
						<span class="relative">
							Login
							<loader v-if="isLoading" class="auth-loader" is-mini />
						</span>
					</button>
				</div>
			</form>
		</template>

		<template #footer>
			<router-link
				class="no-underline hover:underline text-blue-dark text-xs"
				to="/password/forget"
			>
				Forgot Your Password?
			</router-link>
			<br />
			<router-link
				class="no-underline hover:underline text-blue-dark text-xs"
				to="/register"
			>
				Don't you have an account?
			</router-link>
		</template>
	</AuthBase>
</template>

<script setup lang="ts">
	import { useRouter } from 'vue-router';
	import { ref } from 'vue';
	import store from 'src/store';
	import { Login, login as loginAction } from 'src/actions/tmgr/auth';
	import { getUser, getUserSettings } from 'src/actions/tmgr/user';
	import { AxiosError } from 'axios';
	import AuthBase from 'src/components/Auth/AuthBase';
	import { getWorkspaceStatuses } from 'src/actions/tmgr/workspaces';

	const router = useRouter();

	const form = ref({
		email: '',
		password: '',
	} as Login);

	// @todo for savayer only. Find out how to make it global
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
