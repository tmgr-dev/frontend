<template>
	<teleport to="title">
		Reset Password
	</teleport>
	<AuthBase>
		<template #title>Reset Password</template>
		<template #body>
			<form v-if="!message" action="#" class="form-horizontal w-3/4 mx-auto" method="POST">
				<div class="flex flex-col mt-4">
					<input-field
						id="password"
						v-model="password"
						:errors="errors?.password"
						name="password"
						placeholder="Password"
						required
						type="password"
					/>
				</div>
				<div class="flex flex-col mt-4">
					<input-field
						id="password_confirmation"
						v-model="passwordConfirmation"
						:errors="errors?.password_confirmation"
						name="password_confirmation"
						placeholder="Password confirmation"
						required
						type="password"
					/>
				</div>
				<div class="flex flex-col mt-6">
					<button
						class="bg-blue-500 hover:bg-blue-700 text-white text-sm font-semibold py-2 px-4 rounded"
						type="submit"
						@click.prevent="resetPassword">
						<span class="relative">
							Reset
							<loader v-if="isLoading" class="auth-loader" is-mini />
						</span>
					</button>
				</div>
			</form>
			<p v-else>
				{{ message }}
			</p>
		</template>
		<template #footer>
			<router-link class="no-underline hover:underline text-blue-dark text-xs" to="/register">
				You don't have account?
			</router-link>
			<br>
			<router-link class="no-underline hover:underline text-blue-dark text-xs" to="/login">
				Login
			</router-link>
		</template>
	</AuthBase>
</template>

<script>
import AuthBase from 'src/components/Auth/AuthBase';

export default {
	name: 'ResetPassword',
	components: {
		AuthBase
	},
	data: () => ({
		token: null,
		password: null,
		message: null,
		errors: {},
		passwordConfirmation: null,
		isLoading: false
	}),
	methods: {
		async resetPassword() {
			try {
				this.isLoading = true;
				await this.$axios.post(`password/reset/${this.token}`, {
					password: this.password,
					password_confirmation: this.passwordConfirmation
				});
				this.message = 'Your password changed now you can log in with your new password.';
			} catch ({ response: { data: { errors } } }) {
				this.errors = errors;
			} finally {
				this.isLoading = false;
			}
		}
	},
	created() {
		const params = new URLSearchParams(document.location.search);
		this.token = params.get('token');
	}
};
</script>
