<template>
	<teleport to="title"> Registration </teleport>

	<AuthBase>
		<template #title>Hi there!</template>

		<template #body>
			<form action="#" class="form-horizontal mx-auto w-3/4" method="POST">
				<div class="mt-4 flex flex-col">
					<TextField
						v-model="form.name"
						:errors="errors.name"
						name="name"
						input-class="dark:bg-white dark:border-neutral-300"
						placeholder="Name"
					/>
				</div>

				<div class="mt-4 flex flex-col">
					<TextField
						v-model="form.email"
						:errors="errors.email"
						name="email"
						placeholder="E-mail"
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

				<div class="mt-4 flex flex-col">
					<TextField
						v-model="form.password_confirmation"
						:errors="errors.password"
						name="password_confirmation"
						placeholder="Password confirmation"
						input-class="dark:bg-white dark:border-neutral-300"
						type="password"
					/>
				</div>

				<div class="mt-6 flex flex-col">
					<button
						class="rounded bg-blue-500 py-2 px-4 text-sm font-semibold text-white hover:bg-blue-700"
						type="submit"
						@click.prevent="register"
					>
						<span class="relative">
							Register
							<loader v-if="isLoading" class="auth-loader" is-mini />
						</span>
					</button>
				</div>
			</form>
		</template>

		<template #footer>
			<router-link
				class="text-blue-dark text-xs no-underline hover:underline"
				to="/login"
			>
				Do you have an account already?
			</router-link>
		</template>
	</AuthBase>
</template>

<script setup lang="ts">
	import { useRouter } from 'vue-router';
	import { ref } from 'vue';
	import { Register, register as registerAction } from 'src/actions/tmgr/auth';
	import { AxiosError } from 'axios';
	import { getUser } from 'src/actions/tmgr/user';
	import AuthBase from 'src/components/Layouts/AuthBase.vue';
	import TextField from 'src/components/general/TextField.vue';

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

			await router.push({ name: 'CurrentTasksList' });
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
