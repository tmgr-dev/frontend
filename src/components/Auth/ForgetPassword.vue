<template>
	<teleport to="title">
		Restore Password
	</teleport>
	<AuthBase>
		<template #title>Restore Password</template>
		<template #body>
			<form class="form-horizontal w-3/4 mx-auto" method="POST" action="#">
				<div class="flex flex-col mt-4">
					<input id="email" type="text" class="flex-grow h-8 px-2 border rounded border-grey-400" name="email" v-model="email"
								 placeholder="Email">
				</div>
				<div class="flex flex-col mt-6">
					<button
					 	class="bg-blue-500 text-center hover:bg-blue-700 text-white text-sm font-semibold py-2 px-4 rounded"
						@click.prevent="sendResetLink"
					>
						Send link
					</button>
				</div>
			</form>
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
	import Button from "components/UIElements/Button";

	export default {
		name: 'ForgetPassword',
		components: {
			Button,
			AuthBase
		},
		props: [],
		data () {
			return {
				email: null
			}
		},
		computed: {

		},
		mounted () {

		},
		methods: {
			async sendResetLink() {
				const r = await this.$axios.post('password/reset', {
					email: this.email
				})
				console.log(r)
			}
		}
	}
</script>
