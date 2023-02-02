<template>
	<teleport to="title">Profile</teleport>

	<BaseLayout>
		<template #header>Profile</template>

		<template #body>
			<div class="flex flex-col gap-3 max-w-lg">
				<div>
					<input-field
						v-model="user.name"
						:errors="errors?.name"
						class="pb-2"
						placeholder="Your name"
					/>
				</div>

				<div class="w-full flex">
					<input-field
						v-model="user.password"
						:errors="errors?.password"
						class="w-1/2 pr-2"
						placeholder="New password"
					/>

					<input-field
						v-model="user.password_confirmation"
						:errors="errors?.password_confirmation"
						class="w-1/2 pl-2"
						placeholder="New password confirmation"
					/>
				</div>

				<div class="text-left">
					<button
						class="bg-blue-500 hover:bg-blue-700 text-white font-bold py-2 px-8 rounded focus:outline-none sm:mb-0 mt-4"
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
	import Button from 'src/components/UIElements/Button';
	import InputField from 'src/components/UIElements/InputField';
	import { getUser, updateUser } from 'src/actions/tmgr/user';

	export default {
		name: 'Profile',
		components: {
			InputField,
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
