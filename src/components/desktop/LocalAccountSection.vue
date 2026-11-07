<template>
	<SettingsSection title="Account">
		<p class="text-sm text-ink-subtle">
			You are not signed in. Your workspaces live only on this computer.
		</p>
		<template #footer>
			<Button @click="goRegister">Create account</Button>
			<Button variant="outline" @click="goLogin">Sign in</Button>
		</template>
	</SettingsSection>
</template>

<script lang="ts">
	import SettingsSection from '@/components/layouts/SettingsSection.vue';
	import { Button } from '@/components/ui/button';
	import { defineComponent } from 'vue';
	import { useRouter } from 'vue-router';

	export default defineComponent({
		name: 'LocalAccountSection',
		components: { SettingsSection, Button },
		setup() {
			const router = useRouter();
			const goRegister = () => router.push({ name: 'Register' });
			const goLogin = () => router.push({ name: 'Login' });
			return { goRegister, goLogin };
		},
	});
</script>
