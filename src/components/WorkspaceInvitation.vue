<template>
	<teleport to="title">Invitation to the workspace</teleport>

	<BaseLayout>
		<template #header><span class="mt-10 w-full inline-block text-center">You have been invited to the workspace:</span></template>

		<template #body>
			<div class="flex mx-auto flex-col gap-3 max-w-lg">
				<h2 class="md:text-3xl text-2xl mx-auto mt-6 pt-1 md:pt-0 md:mt-0 text-blue-800 dark:text-white relative md:text-left">
					{{ workspace.name }}
				</h2>
				<button
					@click=""
					class="w-full bg-orange-500 mr-5 mt-5 mb-10 hover:bg-orange-600 transition text-white font-bold py-2 px-4 rounded focus:outline-none"
					type="button"
				>
					Accept
				</button>
			</div>
		</template>
	</BaseLayout>
</template>

<script>
	import Button from 'src/components/UIElements/Button';
	import InputField from 'src/components/UIElements/InputField';

	export default {
		name: 'WorkspaceInvitation',
		components: {
			InputField,
			Button,
		},
		data: () => ({
			workspace: {
				name: "Workspace name"
			},
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
