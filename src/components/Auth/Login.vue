<template>
	<teleport to="title"> Login </teleport>

	<AuthBase>
		<template #title>Welcome back!</template>

		<template #body>
			<form class="form-horizontal mx-auto w-3/4" @submit.prevent="login">
				<div class="mt-4 flex flex-col">
					<div :style="{ color: errors ? 'red' : 'initial' }">
						{{ message }}
					</div>
				</div>

				<div class="mt-4 flex flex-col">
					<TextField
						v-model="form.email"
						:errors="errors.email"
						name="email"
						placeholder="Email"
						input-class="dark:bg-white dark:border-neutral-300"
						type="email"
					/>
				</div>

				<div class="mt-4 flex flex-col">
					<TextField
						v-model="form.password"
						:errors="errors.password"
						name="password"
						placeholder="Password"
						input-class="dark:bg-white dark:border-neutral-300"
						type="password"
					/>
				</div>

				<div class="mt-6 flex flex-col">
					<button
						class="rounded bg-blue-500 py-2 px-4 text-sm font-semibold text-white hover:bg-blue-700"
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
				class="text-blue-dark text-xs no-underline hover:underline"
				to="/password/forget"
			>
				Forgot Your Password?
			</router-link>
			<br />
			<router-link
				class="text-blue-dark text-xs no-underline hover:underline"
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
	import AuthBase from 'src/components/Layouts/AuthBase.vue';
	import { getWorkspaceStatuses } from 'src/actions/tmgr/workspaces';
	import TextField from 'src/components/general/TextField.vue';

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
