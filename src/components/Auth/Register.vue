<template>
	<teleport to="title">
		Registration
	</teleport>
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
						class="bg-blue-500 hover:bg-blue-700 text-white text-sm font-semibold py-2 px-4 rounded" type="submit"
						@click.prevent="register">
						<span class="relative">
							Register
							<loader v-if="showLoader" class="auth-loader" is-mini />
						</span>
					</button>
				</div>
			</form>
		</template>
		<template #footer>
			<router-link class="no-underline hover:underline text-blue-dark text-xs" to="/login">
				Do you have an account already?
			</router-link>
		</template>
	</AuthBase>
</template>

<script>
import AuthBase from 'src/components/Auth/AuthBase';

export default {
	name: 'Register',
	components: {
		AuthBase
	},
	props: [],
	data() {
		return {
			showLoader: false,
			form: {
				name: null,
				email: null,
				password: null,
				password_confirmation: null
			},
			errors: {}
		};
	},
	methods: {
		async register() {
			const { ...registerData } = this.form;
			try {
				this.showLoader = true;
				const { data } = await this.$axios.post('auth/register', registerData);
				this.showLoader = false;

				this.$store.commit('token', data.data);
				this.setUser();

			} catch (error) {
				this.showLoader = false;

				if (error && error.response) {
					this.errors = error.response.data.errors;
				}
			}
		},

		async setUser() {
			this.$axios.defaults.headers = {
				Authorization: `Bearer ${this.$store.getters.token.token}`,
				'X-Requested-With': 'XMLHttpRequest'
			};
			const { data } = await this.$axios.get('user');

			this.$store.commit('user', data);
			this.$router.push({ name: 'CurrentTasksList' });
		}
	}
};
</script>
