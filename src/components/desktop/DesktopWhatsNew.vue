<template>
	<Modal
		v-if="state.open"
		modal-class="w-full max-w-lg"
		@close="close"
		@closing-modal="close"
	>
		<template #modal-body>
			<div
				class="flex max-h-[85vh] flex-col text-foreground"
				data-testid="whats-new-modal"
			>
				<div
					class="flex items-center justify-between gap-3 border-b border-border px-5 py-4"
				>
					<h2 class="text-base font-semibold">{{ title }}</h2>
					<button
						type="button"
						title="Close"
						class="rounded-md p-1 text-muted-foreground hover:bg-muted hover:text-foreground"
						@click="close"
					>
						<X class="h-4 w-4" />
					</button>
				</div>

				<div class="max-h-[70vh] space-y-6 overflow-y-auto px-5 py-4">
					<section v-for="section in state.sections" :key="section.version">
						<div class="mb-2 flex items-baseline gap-2">
							<h3 class="text-sm font-semibold">v{{ section.version }}</h3>
							<span v-if="section.date" class="text-xs text-muted-foreground">
								{{ section.date }}
							</span>
						</div>
						<MarkdownText :content="section.body" :link-task-keys="false" />
					</section>
					<p
						v-if="state.sections.length === 0"
						class="text-sm text-muted-foreground"
					>
						No release notes for this version.
					</p>
				</div>

				<div class="flex justify-end border-t border-border px-5 py-3">
					<button
						type="button"
						class="rounded-md bg-primary px-4 py-1.5 text-sm font-medium text-primary-foreground hover:bg-primary/90"
						@click="close"
					>
						Got it
					</button>
				</div>
			</div>
		</template>
	</Modal>
</template>

<script>
	import MarkdownText from '@/components/general/MarkdownText.vue';
	import Modal from '@/components/Modal.vue';
	import router from '@/router';
	import store from '@/store';
	import {
		checkWhatsNewOnStartup,
		closeWhatsNew,
		hadSessionAtLaunch,
		whatsNewState,
	} from '@/utils/whatsNewState';
	import { X } from 'lucide-vue-next';
	import { computed, defineComponent, onMounted, watch } from 'vue';

	export default defineComponent({
		name: 'DesktopWhatsNew',
		components: { MarkdownText, Modal, X },
		setup() {
			let checked = false;
			const hasPriorData = hadSessionAtLaunch();

			const ready = () =>
				store.getters.isLoggedIn && router.currentRoute.value.path !== '/';

			const check = () => {
				if (checked || !ready()) return;
				checked = true;
				checkWhatsNewOnStartup(hasPriorData);
			};

			onMounted(async () => {
				await router.isReady();
				watch(
					() => [
						store.getters.isLoggedIn,
						router.currentRoute.value.path,
					],
					check,
					{ immediate: true },
				);
			});

			return {
				state: whatsNewState,
				close: closeWhatsNew,
				title: computed(() =>
					whatsNewState.value.version
						? `What's new in ${whatsNewState.value.version}`
						: `What's new`,
				),
			};
		},
	});
</script>
