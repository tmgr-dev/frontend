<template>
	<teleport to="title">
		Reset Password
	</teleport>
	<AuthBase>
		<template #title>Reset Password</template>
		<template #body>
			<form v-if="!message" class="form-horizontal w-3/4 mx-auto" method="POST" action="#">
				<div class="flex flex-col mt-4">
					<input-field
						id="password"
						type="password"
						name="password"
						v-model="password"
						required
						placeholder="Password"
						:errors="errors?.password"
					/>
				</div>
				<div class="flex flex-col mt-4">
					<input-field
						id="password_confirmation"
						type="password"
					  name="password_confirmation"
						required
						v-model="passwordConfirmation"
						placeholder="Password confirmation"
						:errors="errors?.password_confirmation"
					/>
				</div>
				<div class="flex flex-col mt-6">
					<button type="submit"
									@click.prevent="resetPassword"
									class="bg-blue-500 hover:bg-blue-700 text-white text-sm font-semibold py-2 px-4 rounded">
						Reset
					</button>
				</div>
			</form>
			<p v-else>
				{{ message }}
			</p>
		</template>
		<template #footer>
			<router-link to="/register" class="no-underline hover:underline text-blue-dark text-xs">
				You don't have account?
			</router-link>
			<br>
			<router-link to="/login" class="no-underline hover:underline text-blue-dark text-xs">
				Login
			</router-link>
		</template>
	</AuthBase>
</template>

<script>

	import AuthBase from "src/components/Auth/AuthBase";

	export default {
		name: 'ResetPassword',
		components: {
			AuthBase
		},
		props: [],
		data () {
			return {
				token: null,
				password: null,
				message: null,
				errors: {},
				passwordConfirmation: null
			}
		},
		computed: {

		},
		created () {
			const params = new URLSearchParams(document.location.search)
			this.token = params.get('token')
		},
		methods: {
			async resetPassword() {

				try {
					await this.$axios.post(`password/reset/${this.token}`, {
						password: this.password,
						password_confirmation: this.passwordConfirmation,
					})
					this.message = 'Your password changed now you can log in with your new password.'
				} catch ({response: {data: {errors}}}) {
					this.errors = errors
				}
			}
		}
	}
</script>
