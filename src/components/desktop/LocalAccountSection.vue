<template>
	<SettingsSection :title="$t('desktop.localAccount.title')">
		<p class="text-sm text-ink-subtle">
			{{ $t('desktop.localAccount.notSignedIn') }}
		</p>
		<template #footer>
			<Button @click="goRegister">{{
				$t('desktop.localAccount.createAccount')
			}}</Button>
			<Button variant="outline" @click="goLogin">{{
				$t('desktop.localAccount.signIn')
			}}</Button>
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
