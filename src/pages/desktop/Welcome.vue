<template>
	<div
		class="flex min-h-screen flex-col items-center justify-center gap-6 bg-background px-6 text-foreground"
	>
		<h1 class="text-2xl font-semibold tracking-tight">Welcome to TMGR</h1>
		<div class="flex flex-wrap items-center justify-center gap-3">
			<Button :disabled="starting" @click="startWithoutAccount">
				Start without an account
			</Button>
			<Button variant="outline" :disabled="starting" @click="signIn">
				Sign in
			</Button>
		</div>
		<p v-if="error" role="alert" class="text-sm text-destructive">
			{{ error }}
		</p>
	</div>
</template>

<script lang="ts">
	import { Button } from '@/components/ui/button';
	import store from '@/store';
	import { defineComponent, ref } from 'vue';
	import { useRouter } from 'vue-router';

	export default defineComponent({
		name: 'Welcome',
		components: { Button },
		setup() {
			const router = useRouter();
			const starting = ref(false);
			const error = ref('');

			const startWithoutAccount = async () => {
				if (starting.value) return;
				starting.value = true;
				error.value = '';
				try {
					await store.dispatch('startWithoutAccount', {});
				} catch (e) {
					console.error('Failed to start without an account', e);
					error.value = 'Could not set up a local workspace. Please try again.';
				} finally {
					starting.value = false;
				}
			};

			const signIn = () => {
				void router.push({ name: 'Login' });
			};

			return { starting, error, startWithoutAccount, signIn };
		},
	});
</script>
