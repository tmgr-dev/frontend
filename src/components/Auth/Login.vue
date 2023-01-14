<template>
	<teleport to="title">
		Login
	</teleport>
	<AuthBase>
		<template #title>Welcome back!</template>
		<template #body>
			<form class="form-horizontal w-3/4 mx-auto" @submit.prevent="login">
				<div class="flex flex-col mt-4">
					<h3 :style="{color: errors ? 'red' : 'initial'}">{{ message }}</h3>
					<p v-if="errors instanceof String" :style="{color: 'red'}">{{ errors }}</p>
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
						type="submit">
						<span class="relative">
							Login
							<loader v-if="showLoader" class="auth-loader" is-mini />
						</span>
					</button>
				</div>
			</form>
		</template>
		<template #footer>
			<router-link class="no-underline hover:underline text-blue-dark text-xs" to="/password/forget">
				Forgot Your Password?
			</router-link>
			<br>
			<router-link class="no-underline hover:underline text-blue-dark text-xs" to="/register">
				Don't you have an account?
			</router-link>
		</template>
	</AuthBase>
</template>

<script>
import AuthBase from 'src/components/Auth/AuthBase';

export default {
	name: 'Login',
	components: {
		AuthBase
	},
	data() {
		return {
			showLoader: false,
			form: {
				email: null,
				password: null
			},
			message: '',
			errors: {}
		};
	},
	methods: {
		async login() {
			const { ...loginData } = this.form;

			try {
				this.showLoader = true;
				const { data: { data } } = await this.$axios.post('auth/login', loginData);

				this.$store.commit('token', data);
				await this.setUser();
				window.location.reload();
			} catch ({ response }) {
				this.errors = response.data.errors;
				this.message = response.data.message;
			}
			this.showLoader = false;
		},
		async setUser() {
			this.$axios.defaults.headers = {
				Authorization: `Bearer ${this.$store.getters.token.token}`,
				'X-Requested-With': 'XMLHttpRequest'
			};
			const { data: { data } } = await this.$axios.get('user');

			this.$store.commit('user', data);
			await this.$router.push({ name: 'CurrentTasksList' });
		}
	}
};
</script>

<style lang="scss">
.auth-form {
	color: #3c3c3c !important;
}
</style>
