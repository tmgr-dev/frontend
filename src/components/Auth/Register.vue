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
							<loader v-if="showLoader" class="auth-loader" is-mini />
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

<script setup>
	import AuthBase from 'src/components/Auth/AuthBase';
	import { ref } from 'vue';
	import userInit from 'src/mixins/UserInitializationMixin';

	const { errors, message, register: defaultRegister } = userInit();
	let showLoader = ref(false);
	const form = ref({
		name: null,
		email: null,
		password: null,
		password_confirmation: null,
	});

	const register = async () => {
		showLoader.value = true;
		await defaultRegister(form);
		showLoader.value = false;
	}
</script>
