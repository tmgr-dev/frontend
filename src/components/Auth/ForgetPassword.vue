<template>
	<teleport to="title">
		Restore Password
	</teleport>
	<AuthBase>
		<template #title>Restore Password</template>
		<template #body>
			<form class="form-horizontal w-3/4 mx-auto" method="POST" action="#">
				<div class="flex flex-col mt-4">
					<input id="email" type="text" class="flex-grow h-8 px-2 border rounded border-grey-400" name="email" value=""
								 placeholder="Email">
				</div>
				<div class="flex flex-col mt-6">
					<router-link to="/password/reset"
											 class="bg-blue-500 text-center hover:bg-blue-700 text-white text-sm font-semibold py-2 px-4 rounded">
						Send link
					</router-link>
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

	export default {
		name: 'ForgetPassword',
		components: {
			AuthBase
		},
		props: [],
		data () {
			return {

			}
		},
		computed: {

		},
		mounted () {

		},
		methods: {

		}
	}
</script>
