<template>
	<teleport to="title">Login</teleport>

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
					<Button
						class="my-1 rounded bg-blue-500 py-2 px-4 text-sm font-semibold text-white hover:bg-blue-700"
						type="submit"
					>
						<span class="relative">
							Login
							<loader v-if="isLoading" class="auth-loader" is-mini />
						</span>
					</Button>
					<Button
						class="my-1 flex justify-around rounded bg-slate-200 py-2 px-4 text-sm font-semibold text-black hover:bg-slate-300"
					>
						<svg
							xmlns="http://www.w3.org/2000/svg"
							width="20"
							height="20"
							viewBox="0 0 32 32"
						>
							<path
								d="M24.734 17.003c-0.040-4.053 3.305-5.996 3.454-6.093-1.88-2.751-4.808-3.127-5.851-3.171-2.492-0.252-4.862 1.467-6.127 1.467-1.261 0-3.213-1.43-5.28-1.392-2.716 0.040-5.221 1.579-6.619 4.012-2.822 4.897-0.723 12.151 2.028 16.123 1.344 1.944 2.947 4.127 5.051 4.049 2.026-0.081 2.793-1.311 5.242-1.311s3.138 1.311 5.283 1.271c2.18-0.041 3.562-1.981 4.897-3.931 1.543-2.255 2.179-4.439 2.216-4.551-0.048-0.022-4.252-1.632-4.294-6.473zM20.705 5.11c1.117-1.355 1.871-3.235 1.665-5.11-1.609 0.066-3.559 1.072-4.713 2.423-1.036 1.199-1.942 3.113-1.699 4.951 1.796 0.14 3.629-0.913 4.747-2.264z"
							></path>
						</svg>
						<span class="relative"> Login with Apple </span>
					</Button>
					<Button
						class="group my-1 flex justify-around rounded bg-black py-2 px-4 text-sm font-semibold text-white hover:bg-gray-800"
					>
						<svg
							class="fill-white"
							xmlns="http://www.w3.org/2000/svg"
							width="20"
							height="20"
							viewBox="0 0 32 32"
						>
							<path
								d="M16 0.395c-8.836 0-16 7.163-16 16 0 7.069 4.585 13.067 10.942 15.182 0.8 0.148 1.094-0.347 1.094-0.77 0-0.381-0.015-1.642-0.022-2.979-4.452 0.968-5.391-1.888-5.391-1.888-0.728-1.849-1.776-2.341-1.776-2.341-1.452-0.993 0.11-0.973 0.11-0.973 1.606 0.113 2.452 1.649 2.452 1.649 1.427 2.446 3.743 1.739 4.656 1.33 0.143-1.034 0.558-1.74 1.016-2.14-3.554-0.404-7.29-1.777-7.29-7.907 0-1.747 0.625-3.174 1.649-4.295-0.166-0.403-0.714-2.030 0.155-4.234 0 0 1.344-0.43 4.401 1.64 1.276-0.355 2.645-0.532 4.005-0.539 1.359 0.006 2.729 0.184 4.008 0.539 3.054-2.070 4.395-1.64 4.395-1.64 0.871 2.204 0.323 3.831 0.157 4.234 1.026 1.12 1.647 2.548 1.647 4.295 0 6.145-3.743 7.498-7.306 7.895 0.574 0.497 1.085 1.47 1.085 2.963 0 2.141-0.019 3.864-0.019 4.391 0 0.426 0.288 0.925 1.099 0.768 6.354-2.118 10.933-8.113 10.933-15.18 0-8.837-7.164-16-16-16z"
							></path>
						</svg>
						<span class="relative"> Login with GitHub </span>
					</Button>
					<Button
						class="my-1 flex justify-around rounded bg-red-400 py-2 px-4 text-sm font-semibold text-white hover:bg-red-700"
					>
						<svg
							width="20"
							height="20"
							viewBox="0 0 533.5 544.3"
							xmlns="http://www.w3.org/2000/svg"
						>
							<path
								d="m533.5 278.4c0-18.5-1.5-37.1-4.7-55.3h-256.7v104.8h147c-6.1 33.8-25.7 63.7-54.4 82.7v68h87.7c51.5-47.4 81.1-117.4 81.1-200.2z"
								style="fill: #4285f4"
							/>
							<path
								d="m272.1 544.3c73.4 0 135.3-24.1 180.4-65.7l-87.7-68c-24.4 16.6-55.9 26-92.6 26-71 0-131.2-47.9-152.8-112.3h-90.5v70.1c46.2 91.9 140.3 149.9 243.2 149.9z"
								style="fill: #34a853"
							/>
							<path
								d="M119.3,324.3c-11.4-33.8-11.4-70.4,0-104.2V150H28.9c-38.6,76.9-38.6,167.5,0,244.4L119.3,324.3z"
								style="fill: #fbbc04"
							/>
							<path
								d="m272.1 107.7c38.8-0.6 76.3 14 104.4 40.8l77.7-77.7c-49.2-46.2-114.5-71.6-182.1-70.8-102.9 0-197 58-243.2 150l90.4 70.1c21.5-64.5 81.8-112.4 152.8-112.4z"
								style="fill: #ea4335"
							/>
						</svg>
						<span class="relative"> Login with Google </span>
					</Button>
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
	import AuthBase from 'src/components/layouts/AuthBase.vue';
	import { getWorkspaceStatuses } from 'src/actions/tmgr/workspaces';
	import TextField from 'src/components/general/TextField.vue';
	import Button from 'src/components/general/Button.vue';

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
