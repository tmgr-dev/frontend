<template>
	<teleport to="title">
		Profile
	</teleport>
	<BaseLayout>
		<template #header>
			Profile
		</template>
		<template #body>
			<div :class="`w-full md:w-auto md:flex-grow md:flex md:items-center md:block block ${$color('textMain')}`">
				<div>
					<input-field
						v-model="user.name"
						:errors="errors?.name"
						class="pb-2"
						placeholder="Enter your name"
					/>
				</div>
				<div class="w-full flex">
					<input-field
						v-model="user.password"
						:errors="errors?.password"
						class="w-1/2 pr-2"
						placeholder="Enter new password"
					/>
					<input-field
						v-model="user.password_confirmation"
						:errors="errors?.password_confirmation"
						class="w-1/2 pl-2"
						placeholder="Enter new password confirmation"
					/>
				</div>
				<div class="text-right">
					<button
						class="bg-blue-500 hover:bg-blue-700 text-white font-bold py-2 px-4 rounded focus:outline-none sm:mb-0 mt-10"
						type="button"
						@click="saveUser">
						Save
					</button>
				</div>
			</div>
		</template>
	</BaseLayout>
	<confirm v-if="confirm" :body="confirm.body" :title="confirm.title" @onCancel="confirm = undefined"
					 @onOk="confirm.action()">
		<template #body-content>
			<p>{{ confirm.body }}</p>
			<a class="text-blue" href="/push-notifications-enable-guide" target="_blank">How to enable or disable Push
				Notifications on Chrome, Firefox & Safari Browser?</a>
		</template>
	</confirm>
</template>

<script>
import Button from 'src/components/UIElements/Button';
import InputField from 'components/UIElements/InputField';

export default {
	name: 'Profile',
	components: {
		InputField,
		Button
	},
	data: () => ({
		user: {
			name: null,
			password: null,
			password_confirmation: null
		},
		errors: {}
	}),
	async mounted() {
		await this.loadUser();
	},
	methods: {
		async loadUser() {
			const { data: { data } } = await this.$axios.get('user');
			this.user = data;
		},
		async saveUser() {
			try {
				const { data: { data } } = await this.$axios.put('user', this.user);
				this.user = data;
				this.showAlert('Saved', 'User data saved');
			} catch ({ response: { data: { errors } } }) {
				this.errors = errors;
			}
		}
	}
};
</script>

<style scoped>
.settings-container {
	max-width: 700px;
	margin: 50px auto;
	padding: 20px;
	box-shadow: rgb(233 233 233) 1px 4px 20px;
}
</style>
