<template>
	<div v-if="isDemo || showFirstRun || warning" class="flex flex-col">
		<div
			v-if="isDemo"
			role="status"
			data-testid="demo-banner"
			class="flex flex-wrap items-center gap-x-3 gap-y-1 border-b border-violet-300 bg-violet-100 px-4 py-2 text-sm text-violet-900 dark:border-violet-500/30 dark:bg-violet-500/10 dark:text-violet-200"
		>
			<span class="min-w-0 flex-1">{{
				$t('desktop.localOnlyBanner.demo')
			}}</span>
			<span v-if="deleteError" role="alert" class="text-destructive">
				{{ deleteError }}
			</span>
			<button
				v-if="onEntryFlow"
				type="button"
				data-testid="demo-sign-in"
				class="rounded-md px-2 py-1 font-medium underline underline-offset-4 hover:bg-violet-200 dark:hover:bg-violet-500/20"
				@click="$router.push({ name: 'Login' })"
			>
				{{ $t('desktop.localOnlyBanner.signIn') }}
			</button>
			<template v-if="confirmingDelete">
				<span>{{ $t('desktop.localOnlyBanner.deleteDemoQuestion') }}</span>
				<button
					type="button"
					data-testid="demo-delete-confirm"
					class="rounded-md px-2 py-1 font-medium underline underline-offset-4 hover:bg-violet-200 disabled:opacity-60 dark:hover:bg-violet-500/20"
					:disabled="deleting"
					@click="confirmDelete"
				>
					{{
						deleting
							? $t('desktop.localOnlyBanner.deleting')
							: $t('common.states.yes')
					}}
				</button>
				<button
					type="button"
					data-testid="demo-delete-cancel"
					class="rounded-md px-2 py-1 hover:bg-violet-200 disabled:opacity-60 dark:hover:bg-violet-500/20"
					:disabled="deleting"
					@click="confirmingDelete = false"
				>
					{{ $t('common.actions.cancel') }}
				</button>
			</template>
			<button
				v-else
				type="button"
				data-testid="demo-delete"
				class="rounded-md px-2 py-1 font-medium underline underline-offset-4 hover:bg-violet-200 disabled:opacity-60 dark:hover:bg-violet-500/20"
				:disabled="deleting"
				@click="confirmingDelete = true"
			>
				{{ $t('desktop.localOnlyBanner.deleteDemo') }}
			</button>
		</div>
		<div
			v-if="showFirstRun"
			role="status"
			data-testid="local-only-banner"
			data-tour="local.banner"
			class="flex flex-wrap items-center gap-x-3 gap-y-1 border-b border-sky-300 bg-sky-100 px-4 py-2 text-sm text-sky-900 dark:border-sky-500/30 dark:bg-sky-500/10 dark:text-sky-200"
		>
			<i18n-t
				keypath="desktop.localOnlyBanner.firstRun"
				tag="span"
				class="min-w-0 flex-1"
			>
				<template #path>
					<strong class="[overflow-wrap:anywhere]" :title="backupsDir">{{
						backupsLabel
					}}</strong>
				</template>
			</i18n-t>
			<button
				type="button"
				class="rounded-md px-2 py-1 font-medium underline underline-offset-4 hover:bg-sky-200 dark:hover:bg-sky-500/20"
				@click="openBackups"
			>
				{{ $t('desktop.localOnlyBanner.openBackups') }}
			</button>
			<button
				type="button"
				class="rounded-md px-2 py-1 font-medium underline underline-offset-4 hover:bg-sky-200 dark:hover:bg-sky-500/20"
				@click="createAccount"
			>
				{{ $t('desktop.localOnlyBanner.createAccount') }}
			</button>
			<button
				type="button"
				class="rounded-md px-2 py-1 hover:bg-sky-200 dark:hover:bg-sky-500/20"
				:aria-label="$t('desktop.localOnlyBanner.dismiss')"
				@click="dismissFirstRun"
			>
				<X class="h-4 w-4" />
			</button>
		</div>
		<div
			v-if="warning"
			role="alert"
			data-testid="backup-warning"
			class="flex flex-wrap items-center gap-x-3 gap-y-1 border-b border-amber-300 bg-amber-100 px-4 py-2 text-sm text-amber-900 dark:border-amber-500/30 dark:bg-amber-500/10 dark:text-amber-200"
		>
			<span class="min-w-0 flex-1 break-words">{{ warning.message }}</span>
			<button
				type="button"
				class="rounded-md px-2 py-1 font-medium underline underline-offset-4 hover:bg-amber-200 disabled:opacity-60 dark:hover:bg-amber-500/20"
				:disabled="backingUp"
				@click="backupNowClick"
			>
				{{
					backingUp
						? $t('desktop.localOnlyBanner.backingUp')
						: $t('desktop.localOnlyBanner.backUpNow')
				}}
			</button>
			<button
				type="button"
				class="rounded-md px-2 py-1 hover:bg-amber-200 dark:hover:bg-amber-500/20"
				:aria-label="$t('desktop.localOnlyBanner.hideForNow')"
				@click="hiddenForSession = true"
			>
				<X class="h-4 w-4" />
			</button>
		</div>
	</div>
</template>

<script lang="ts">
	import { useDemoWorkspace } from '@/composable/useDemoWorkspace';
	import { t } from '@/i18n';
	import {
		backupNow,
		backupStatus,
		revealBackups,
		type BackupStatus,
	} from '@/local/backups';
	import { completeDemoEntry, deleteDemo } from '@/local/demoApi';
	import { ghostStage } from '@/onboarding/ghostStage';
	import { activeLocalWorkspace } from '@/local/runtime';
	import { desktopOfferEnv, offerBackups } from '@/tours/desktopOffers';
	import { isDesktopApp } from '@/utils/desktop';
	import {
		backupWarning,
		bannerDismissed,
		dismissBanner,
		tildePath,
	} from '@/utils/localOnly';
	import { X } from 'lucide-vue-next';
	import {
		computed,
		defineComponent,
		onBeforeUnmount,
		onMounted,
		ref,
		watch,
	} from 'vue';
	import { useRouter } from 'vue-router';
	import { useStore } from 'vuex';

	const REFRESH_MS = 10 * 60_000;

	export default defineComponent({
		name: 'LocalOnlyBanner',
		components: { X },
		setup() {
			const store = useStore();
			const router = useRouter();
			const statuses = ref<BackupStatus[]>([]);
			const firstRunDismissed = ref(bannerDismissed());
			const hiddenForSession = ref(false);
			const backingUp = ref(false);
			const { isDemo, code: demoCode } = useDemoWorkspace();
			const onEntryFlow = computed(() => ghostStage.value !== null);
			const confirmingDelete = ref(false);
			const deleting = ref(false);
			const deleteError = ref('');
			let timer: ReturnType<typeof setInterval> | undefined;

			const localOnly = computed(() => store.getters.isLocalOnly === true);
			const inLocalWorkspace = computed(
				() =>
					localOnly.value || store.getters.currentWorkspace?.is_local === true,
			);
			const watching = computed(() => isDesktopApp() && inLocalWorkspace.value);

			const code = computed(
				() =>
					activeLocalWorkspace()?.code ??
					store.getters.currentWorkspace?.code ??
					null,
			);
			const status = computed(
				() => statuses.value.find((s) => s.code === code.value) ?? null,
			);
			const backupsDir = computed(() => status.value?.backups_dir ?? '');
			const backupsLabel = computed(
				() =>
					tildePath(backupsDir.value) ||
					t('desktop.localOnlyBanner.backupsFolder'),
			);

			const refresh = async () => {
				if (!watching.value) return;
				try {
					statuses.value = await backupStatus();
				} catch {
					return;
				}
			};

			onMounted(() => {
				void refresh();
				timer = setInterval(refresh, REFRESH_MS);
			});
			onBeforeUnmount(() => clearInterval(timer));
			watch(watching, () => void refresh());

			const showFirstRun = computed(
				() =>
					isDesktopApp() &&
					localOnly.value &&
					!isDemo.value &&
					!firstRunDismissed.value,
			);
			const warning = computed(() => {
				if (!watching.value || hiddenForSession.value || !status.value)
					return null;
				return backupWarning(status.value);
			});

			watch(
				warning,
				(value) => {
					if (value) offerBackups(desktopOfferEnv());
				},
				{ immediate: true },
			);

			const dismissFirstRun = () => {
				dismissBanner();
				firstRunDismissed.value = true;
			};
			const openBackups = () => {
				if (status.value) void revealBackups(status.value.code).catch(() => {});
			};
			const createAccount = () => router.push({ name: 'Register' });
			const backupNowClick = async () => {
				if (!status.value || backingUp.value) return;
				backingUp.value = true;
				try {
					await backupNow(status.value.code);
				} catch {
					return;
				} finally {
					backingUp.value = false;
					await refresh();
				}
			};

			const confirmDelete = async () => {
				if (!demoCode.value || deleting.value) return;
				deleting.value = true;
				deleteError.value = '';
				try {
					if (ghostStage.value === 'watch' || ghostStage.value === 'wheel')
						await completeDemoEntry({ deleteDemo: true });
					else await deleteDemo(demoCode.value);
				} catch (e) {
					deleteError.value = e instanceof Error ? e.message : String(e);
					confirmingDelete.value = false;
				} finally {
					deleting.value = false;
				}
			};

			return {
				isDemo,
				onEntryFlow,
				showFirstRun,
				warning,
				backupsDir,
				backupsLabel,
				hiddenForSession,
				backingUp,
				confirmingDelete,
				deleting,
				deleteError,
				dismissFirstRun,
				openBackups,
				createAccount,
				backupNowClick,
				confirmDelete,
			};
		},
	});
</script>
