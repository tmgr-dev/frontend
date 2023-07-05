<template>
	<teleport to="title">Profile</teleport>

	<BaseLayout>
		<!--		<template #header>Profile</template>-->

		<template #body>
			<div class="flex max-w-lg flex-col gap-3">
				<TextField
					v-model="user.name"
					:errors="errors.name"
					placeholder="Your name"
				/>

				<div class="flex">
					<TextField
						v-model="user.password"
						:errors="errors?.password"
						class="w-1/2 pr-2"
						placeholder="New password"
					/>

					<TextField
						v-model="user.password_confirmation"
						:errors="errors.password_confirmation"
						class="w-1/2 pl-2"
						placeholder="New password confirmation"
					/>
				</div>

				<div class="text-left">
					<button
						class="mt-4 rounded bg-blue-500 py-2 px-8 font-bold text-white hover:bg-blue-700 focus:outline-none sm:mb-0"
						type="button"
						@click="saveUser"
					>
						Save
					</button>
				</div>
			</div>
		</template>
	</BaseLayout>
</template>

<script>
	import Button from 'src/components/general/Button.vue';
	import TextField from 'src/components/general/TextField.vue';
	import { getUser, updateUser } from 'src/actions/tmgr/user';

	export default {
		name: 'Profile',
		components: {
			TextField,
			Button,
		},
		data: () => ({
			user: {
				name: null,
				password: null,
				password_confirmation: null,
			},
			errors: {},
		}),
		async mounted() {
			// @todo try to get from store this data first
			this.user = await getUser(); // inside we put response to store. @todo think how to reorganize it or don't care
		},
		methods: {
			async saveUser() {
				try {
					this.user = updateUser(this.user);
					this.showAlert('Saved', 'User data saved');
				} catch (error) {
					this.errors = error.response.data.errors;
				}
			},
		},
	};
</script>
