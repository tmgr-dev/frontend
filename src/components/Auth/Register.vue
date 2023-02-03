<template>
	<teleport to="title"> Registration </teleport>

	<AuthBase>
		<template #title>Hi there!</template>

		<template #body>
			<form action="#" class="form-horizontal w-3/4 mx-auto" method="POST">
				<div class="flex flex-col mt-4">
					<input-field
						v-model="form.name"
						:errors="errors.name"
						name="name"
						placeholder="Name"
						type="text"
					/>
				</div>

				<div class="flex flex-col mt-4">
					<input-field
						v-model="form.email"
						:errors="errors.email"
						name="email"
						placeholder="E-mail"
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

				<div class="flex flex-col mt-4">
					<input-field
						v-model="form.password_confirmation"
						:errors="errors.password"
						name="password_confirmation"
						placeholder="Password confirmation"
						type="password"
					/>
				</div>

				<div class="flex flex-col mt-6">
					<button
						class="bg-blue-500 hover:bg-blue-700 text-white text-sm font-semibold py-2 px-4 rounded"
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
				class="no-underline hover:underline text-blue-dark text-xs"
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
	import AuthBase from 'src/components/Auth/AuthBase';

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
