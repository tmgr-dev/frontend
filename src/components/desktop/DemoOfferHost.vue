<template>
	<Dialog :open="open" @update:open="(value: boolean) => !value && decline()">
		<DialogContent class="max-w-md" data-testid="demo-offer">
			<DialogHeader>
				<DialogTitle>Want a quick tour with sample data?</DialogTitle>
				<DialogDescription>
					The demo is a separate workspace with a ready board and a sample agent
					run. It stays on this Mac, never mixes with your own tasks, and you
					can delete it any time.
				</DialogDescription>
			</DialogHeader>
			<p v-if="error" class="text-sm text-destructive">{{ error }}</p>
			<DialogFooter>
				<Button
					variant="outline"
					data-testid="demo-offer-decline"
					:disabled="busy"
					@click="decline"
					>No thanks</Button
				>
				<Button data-testid="demo-offer-accept" :disabled="busy" @click="accept"
					>Watch the demo</Button
				>
			</DialogFooter>
		</DialogContent>
	</Dialog>
</template>

<script lang="ts">
	import { Button } from '@/components/ui/button';
	import {
		Dialog,
		DialogContent,
		DialogDescription,
		DialogFooter,
		DialogHeader,
		DialogTitle,
	} from '@/components/ui/dialog';
	import {
		demoOfferAnswered,
		rememberDemoOfferAnswer,
		shouldOfferDemo,
	} from '@/local/demoOffer';
	import { readFreshMarker } from '@/onboarding/localFirstRun';
	import { defineComponent, onMounted, ref } from 'vue';
	import { useStore } from 'vuex';

	export default defineComponent({
		name: 'DemoOfferHost',
		components: {
			Button,
			Dialog,
			DialogContent,
			DialogDescription,
			DialogFooter,
			DialogHeader,
			DialogTitle,
		},
		setup() {
			const store = useStore();
			const open = ref(false);
			const busy = ref(false);
			const error = ref('');

			onMounted(async () => {
				if (!store.getters.isLocalOnly || !store.getters.canUseApp) return;
				const { listLocalWorkspaces } = await import('@/local/runtime');
				const list = await listLocalWorkspaces();
				open.value = shouldOfferDemo({
					localOnly: store.getters.isLocalOnly,
					canUseApp: store.getters.canUseApp,
					workspaceCount: list.length,
					hasDemo: list.some((workspace) => workspace.demo),
					freshInstall: readFreshMarker(localStorage) !== null,
					answered: demoOfferAnswered(localStorage),
				});
			});

			const decline = () => {
				if (busy.value) return;
				rememberDemoOfferAnswer(localStorage, 'declined');
				open.value = false;
			};

			const accept = async () => {
				busy.value = true;
				error.value = '';
				try {
					const { watchDemo } = await import('@/local/demoApi');
					await watchDemo();
					rememberDemoOfferAnswer(localStorage, 'watched');
					open.value = false;
				} catch (e) {
					error.value = e instanceof Error ? e.message : String(e);
				} finally {
					busy.value = false;
				}
			};

			return { open, busy, error, accept, decline };
		},
	});
</script>
