<template>
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
						type="email"
						name="email"
						:errors="errors.email"
						v-model="form.email"
					 	placeholder="Email"
					/>
				</div>
				<div class="flex flex-col mt-4">
					<input-field type="password" name="password" :errors="errors.password" v-model="form.password"
											 placeholder="Password"/>
				</div>
				<div class="flex items-center mt-4">
					<label class="checkbox-wrapper">
						<input type="checkbox" name="remember" class="mr-2">
						<span class="checkmark"></span>
						<span class="text-sm text-grey-dark select-none">Remember Me</span>
					</label>
				</div>
				<div class="flex flex-col mt-8">
					<button
						type="submit"
						class="bg-blue-500 hover:bg-blue-700 text-white text-sm font-semibold py-2 px-4 rounded">
						<span class="relative">
							Login
							<loader v-if="showLoader" :is-mini="true"/>
						</span>
					</button>
				</div>
			</form>
		</template>
		<template #footer>
			<router-link to="/password/forget" class="no-underline hover:underline text-blue-dark text-xs">
				Forgot Your Password?
			</router-link>
			<br>
			<router-link to="/register" class="no-underline hover:underline text-blue-dark text-xs">
				Don't you have an account?
			</router-link>
		</template>
	</AuthBase>
</template>

<script>
	import AuthBase from "@/components/Auth/AuthBase";

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
					password: null,
				},
				message: '',
				errors: {}
			}
		},
		methods: {
			async login() {
				const { ...loginData } = this.form

				try {
					this.showLoader = true
					const {data} = await this.$axios.post('auth/login', loginData)

					this.$store.commit('token', data.data)
					await this.setUser()

				} catch ({ response }) {
					this.errors = response.data.errors
					this.message = response.data.message
				}
				this.showLoader = false
			},
			async setUser() {
				this.$axios.defaults.headers = {
					Authorization: `Bearer ${this.$store.getters.token.token}`,
					'X-Requested-With': 'XMLHttpRequest'
				}
				const {data} = await this.$axios.get('user')

				this.$store.commit('user', data)
				await this.$router.push({name: 'CurrentTasksList'})
			}
		}
	}
</script>

<style lang="scss">
	.auth-form {
		color: #3c3c3c !important;
	}
</style>
