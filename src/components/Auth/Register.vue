<template>
	<teleport to="title">
		Registration
	</teleport>
	<AuthBase>
		<template #title>Hi there!</template>
		<template #body>
			<form class="form-horizontal w-3/4 mx-auto" method="POST" action="#">
				<div class="flex flex-col mt-4">
					<input-field
						type="text"
						name="name"
						:errors="errors.name"
						v-model="form.name"
						placeholder="Name"
					/>
				</div>
				<div class="flex flex-col mt-4">
					<input-field
						type="email"
						name="email"
						:errors="errors.email"
						v-model="form.email"
						placeholder="E-mail"
					/>
				</div>
				<div class="flex flex-col mt-4">
					<input-field
						type="password"
						name="password"
						:errors="errors.password"
						v-model="form.password"
						placeholder="Password"
					/>
				</div>
				<div class="flex flex-col mt-4">
					<input-field
						type="password"
						name="password_confirmation"
						:errors="errors.password"
						v-model="form.password_confirmation"
						placeholder="Password confirmation"
					/>
				</div>
				<div class="flex flex-col mt-6">
					<button
						type="submit" @click.prevent="register"
						class="bg-blue-500 hover:bg-blue-700 text-white text-sm font-semibold py-2 px-4 rounded">
						<span class="relative">
							Register
							<loader v-if="showLoader" class="auth-loader" is-mini />
						</span>
					</button>
				</div>
			</form>
		</template>
		<template #footer>
			<router-link to="/login" class="no-underline hover:underline text-blue-dark text-xs">
				Do you have an account already?
			</router-link>
		</template>
	</AuthBase>
</template>

<script>
	import AuthBase from "src/components/Auth/AuthBase";

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
					password_confirmation: null,
				},
				errors: {}
			}
		},
		methods: {
			async register() {
				const {...registerData} = this.form
				try {
					this.showLoader = true
					const { data } = await this.$axios.post('auth/register', registerData)
					this.showLoader = false

					this.$store.commit('token', data.data)
					this.setUser()

				} catch (error) {
					this.showLoader = false

					if (error && error.response) {
						this.errors = error.response.data.errors
					}
				}
			},

			async setUser() {
				this.$axios.defaults.headers = {
					Authorization: `Bearer ${this.$store.getters.token.token}`,
					'X-Requested-With': 'XMLHttpRequest'
				}
				const { data } = await this.$axios.get('user')

				this.$store.commit('user', data)
				this.$router.push({ name: 'CurrentTasksList' })
			}
		}
	}
</script>
