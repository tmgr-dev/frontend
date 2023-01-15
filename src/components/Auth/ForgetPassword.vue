<template>
	<teleport to="title"> Restore Password </teleport>
	<AuthBase>
		<template #title>Restore Password</template>
		<template #body>
			<form
				v-if="!message"
				action="#"
				class="form-horizontal w-3/4 mx-auto"
				method="POST"
			>
				<div class="flex flex-col mt-4">
					<input-field
						id="email"
						v-model="email"
						:errors="errors?.email"
						name="email"
						placeholder="Email"
						type="text"
					/>
				</div>
				<div class="flex flex-col mt-6">
					<button
						class="bg-blue-500 text-center hover:bg-blue-700 text-white text-sm font-semibold py-2 px-4 rounded"
						@click.prevent="sendResetLink"
					>
						<span class="relative">
							Send link
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
			<router-link
				class="no-underline hover:underline text-blue-dark text-xs"
				to="/register"
			>
				You don't have account?
			</router-link>
			<br />
			<router-link
				class="no-underline hover:underline text-blue-dark text-xs"
				to="/login"
			>
				Login
			</router-link>
		</template>
	</AuthBase>
</template>

<script>
	import AuthBase from 'src/components/Auth/AuthBase';
	import Button from 'components/UIElements/Button';

	export default {
		name: 'ForgetPassword',
		components: {
			Button,
			AuthBase,
		},
		data: () => ({
			email: null,
			message: null,
			errors: {},
			isLoading: false,
		}),
		methods: {
			async sendResetLink() {
				try {
					this.isLoading = true;
					const r = await this.$axios.post('password/reset', {
						email: this.email,
					});
					this.message =
						"The reset link sent. If you don't have an email with link please try check out your spam.";
				} catch ({
					response: {
						data: { errors },
					},
				}) {
					this.errors = errors;
				} finally {
					this.isLoading = false;
				}
			},
		},
	};
</script>
