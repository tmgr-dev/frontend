<template>
	<AuthLayout>
		<template #title>Welcome to TMGR</template>
		<template #subtitle>
			Tasks, boards and a timer on every task. Start right away or sign in.
		</template>

		<div class="grid grid-cols-2 gap-2">
			<Button
				class="h-auto min-h-10 whitespace-normal px-4 py-2 text-center leading-tight"
				:variant="startOpen ? 'secondary' : 'default'"
				:disabled="busy"
				data-testid="start-local"
				@click="startOpen = true"
			>
				Start without an account
			</Button>
			<Button
				variant="outline"
				class="h-auto min-h-10 whitespace-normal px-4 py-2 text-center leading-tight"
				:disabled="busy"
				data-testid="sign-in"
				@click="signIn"
			>
				Sign in
			</Button>
		</div>

		<p class="mt-4 text-center text-sm text-muted-foreground">
			{{ note }}
		</p>

		<form
			v-if="startOpen"
			class="mt-6 space-y-4"
			data-testid="start-form"
			@submit.prevent="start"
		>
			<div class="space-y-2">
				<Label for="local-name" class="text-foreground">Your name</Label>
				<Input
					id="local-name"
					v-model="name"
					type="text"
					placeholder="Optional"
					autocomplete="name"
					class="h-10"
					:disabled="busy"
				/>
			</div>
			<p v-if="error" role="alert" class="text-sm text-destructive">
				{{ error }}
			</p>
			<Button
				type="submit"
				class="h-10 w-full"
				:disabled="busy"
				:aria-busy="busy"
			>
				<Loader2 v-if="busy" class="animate-spin" />
				{{ busy ? 'Starting…' : 'Continue' }}
			</Button>
		</form>
	</AuthLayout>
</template>

<script lang="ts">
	import AuthLayout from '@/components/auth/AuthLayout.vue';
	import { Button } from '@/components/ui/button';
	import { Input } from '@/components/ui/input';
	import { Label } from '@/components/ui/label';
	import { setDocumentTitle } from '@/composable/useDocumentTitle';
	import { desktopPlatform } from '@/utils/desktop';
	import { welcomeNote } from '@/utils/localOnly';
	import { Loader2 } from 'lucide-vue-next';
	import { defineComponent, onMounted, ref } from 'vue';
	import { useRouter } from 'vue-router';
	import { useStore } from 'vuex';

	export default defineComponent({
		name: 'DesktopWelcome',
		components: { AuthLayout, Button, Input, Label, Loader2 },
		setup() {
			const store = useStore();
			const router = useRouter();
			const startOpen = ref(false);
			const name = ref('');
			const busy = ref(false);
			const error = ref('');
			const note = welcomeNote(desktopPlatform());

			onMounted(() => setDocumentTitle('Welcome'));

			const start = async () => {
				if (busy.value) return;
				busy.value = true;
				error.value = '';
				try {
					await store.dispatch('startWithoutAccount', {
						name: name.value.trim(),
					});
				} catch (e: any) {
					error.value =
						e?.response?.data?.message ||
						e?.message ||
						'Could not start. Try again.';
				} finally {
					busy.value = false;
				}
			};

			const signIn = () => router.push({ name: 'Login' });

			return { startOpen, name, busy, error, note, start, signIn };
		},
	});
</script>
