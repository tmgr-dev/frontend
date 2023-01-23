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
			await this.loadUser();
		},
		methods: {
			async loadUser() {
				const {
					data: { data },
				} = await this.$axios.get('user');
				this.user = data;
			},
			async saveUser() {
				try {
					const {
						data: { data },
					} = await this.$axios.put('user', this.user);
					this.user = data;
					this.showAlert('Saved', 'User data saved');
				} catch ({
					response: {
						data: { errors },
					},
				}) {
					this.errors = errors;
				}
			},
		},
	};
</script>
