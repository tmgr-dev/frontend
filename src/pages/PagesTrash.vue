<template>
	<PageContainer>
		<PageHeader
			:title="$t('pages.trash.title')"
			:back="`/${workspaceCode}/pages`"
			:back-label="$t('pages.trash.back')"
		>
			<template v-if="loaded && !failed && pages.length" #actions>
				<button
					type="button"
					class="inline-flex h-9 items-center gap-2 rounded-md px-3 text-sm text-status-fix-fg hover:bg-surface-hover"
					data-testid="pages-archive-empty"
					@click="openEmpty"
				>
					<Trash2 class="h-4 w-4" />
					{{ $t('pages.trash.deleteAll') }}
				</button>
			</template>
		</PageHeader>
		<p v-if="!loaded" class="text-sm text-ink-subtle">
			{{ $t('common.states.loading') }}
		</p>
		<p v-else-if="failed" class="text-sm text-destructive">
			{{ $t('pages.trash.failed') }}
		</p>
		<p v-else-if="!pages.length" class="text-sm text-ink-subtle">
			{{ $t('pages.trash.empty') }}
		</p>
		<ul
			v-else
			class="divide-y divide-line overflow-hidden rounded-card border border-line bg-surface"
		>
			<li
				v-for="page in pages"
				:key="page.id"
				class="flex flex-wrap items-center gap-x-3 gap-y-2 px-4 py-3"
				:data-page-id="page.id"
			>
				<Archive class="h-4 w-4 shrink-0 text-ink-subtle" />
				<div class="min-w-0 flex-1 basis-48">
					<p class="truncate text-sm font-medium text-ink">{{ page.title }}</p>
					<p
						class="flex min-w-0 flex-wrap items-center gap-x-1.5 text-xs text-ink-subtle"
					>
						<span>{{
							$t('pages.trash.archivedAt', {
								date: formatDeleted(page.deleted_at),
							})
						}}</span>
						<template v-if="page.author?.name">
							<span aria-hidden="true">·</span>
							<AuthorBadge
								:author="toAuthorRef(page.author)"
								:size="16"
								data-testid="pages-archive-author"
							/>
						</template>
					</p>
				</div>
				<div class="ml-auto flex shrink-0 gap-2 whitespace-nowrap">
					<span
						class="inline-flex"
						:title="
							page.can_restore === false
								? $t('pages.trash.restoreForbidden')
								: undefined
						"
					>
						<button
							type="button"
							class="inline-flex h-8 items-center gap-1.5 rounded-md border border-input px-3 text-sm text-ink hover:bg-surface-hover disabled:cursor-not-allowed disabled:opacity-50"
							:disabled="restoring === page.id || page.can_restore === false"
							@click="restore(page)"
						>
							<Undo2 class="h-4 w-4" />
							{{ $t('pages.trash.restore') }}
						</button>
					</span>
					<span
						class="inline-flex"
						:title="
							page.can_delete === false
								? $t('pages.trash.deleteForbidden')
								: undefined
						"
					>
						<button
							type="button"
							class="inline-flex h-8 items-center gap-1.5 rounded-md border border-status-fix px-3 text-sm text-status-fix-fg hover:bg-surface-hover disabled:cursor-not-allowed disabled:opacity-50"
							:disabled="busy || page.can_delete === false"
							@click="purging = page"
						>
							<Trash2 class="h-4 w-4" />
							{{ $t('common.actions.delete') }}
						</button>
					</span>
				</div>
			</li>
		</ul>

		<AlertDialog
			:open="!!purging"
			@update:open="(open) => !open && !busy && (purging = null)"
		>
			<AlertDialogContent>
				<AlertDialogHeader>
					<AlertDialogTitle>{{
						$t('pages.trash.purgeTitle')
					}}</AlertDialogTitle>
					<AlertDialogDescription class="[overflow-wrap:anywhere]">
						{{ $t('pages.trash.purgeBody', { title: purging?.title ?? '' }) }}
					</AlertDialogDescription>
				</AlertDialogHeader>
				<AlertDialogFooter>
					<AlertDialogCancel :disabled="busy">{{
						$t('common.actions.cancel')
					}}</AlertDialogCancel>
					<Button variant="destructive" :disabled="busy" @click="confirmPurge">
						{{ $t('pages.trash.purgeConfirm') }}
					</Button>
				</AlertDialogFooter>
			</AlertDialogContent>
		</AlertDialog>

		<AlertDialog
			:open="emptyOpen"
			@update:open="(open) => !open && !busy && (emptyOpen = false)"
		>
			<AlertDialogContent>
				<form class="grid gap-4" @submit.prevent="confirmEmpty">
					<AlertDialogHeader>
						<AlertDialogTitle>{{
							$t('pages.trash.emptyTitle')
						}}</AlertDialogTitle>
						<AlertDialogDescription>
							{{ $t('pages.trash.emptyBody', { label: countLabel }) }}
						</AlertDialogDescription>
					</AlertDialogHeader>
					<label class="grid gap-1.5 text-sm text-ink">
						<i18n-t keypath="pages.trash.confirmType" tag="span">
							<template #word>
								<span class="font-semibold">{{ CONFIRM_WORD }}</span>
							</template>
						</i18n-t>
						<input
							v-model="confirmWord"
							data-selectable
							autocomplete="off"
							class="h-9 rounded-md border border-input bg-background px-3 text-sm outline-hidden"
						/>
					</label>
					<AlertDialogFooter>
						<AlertDialogCancel type="button" :disabled="busy">
							{{ $t('common.actions.cancel') }}
						</AlertDialogCancel>
						<Button
							type="submit"
							variant="destructive"
							:disabled="busy || !confirmed"
						>
							{{ $t('pages.trash.deleteAll') }}
						</Button>
					</AlertDialogFooter>
				</form>
			</AlertDialogContent>
		</AlertDialog>
	</PageContainer>
</template>

<script lang="ts">
	import {
		emptyPagesTrash,
		getPagesTrash,
		permanentlyDeletePage,
		restorePage,
		type TrashedPage,
	} from '@/actions/tmgr/pages';
	import AuthorBadge from '@/components/general/AuthorBadge.vue';
	import PageContainer from '@/components/layouts/PageContainer.vue';
	import PageHeader from '@/components/layouts/PageHeader.vue';
	import {
		AlertDialog,
		AlertDialogCancel,
		AlertDialogContent,
		AlertDialogDescription,
		AlertDialogFooter,
		AlertDialogHeader,
		AlertDialogTitle,
	} from '@/components/ui/alert-dialog';
	import { Button } from '@/components/ui/button';
	import { useToast } from '@/components/ui/toast';
	import { currentLocale, t } from '@/i18n';
	import { toAuthorRef } from '@/utils/pages/author';
	import { pageUrl } from '@/utils/pagesTree';
	import { Archive, Trash2, Undo2 } from 'lucide-vue-next';
	import { computed, defineComponent, onMounted, ref, watch } from 'vue';
	import { useRoute, useRouter } from 'vue-router';
	import { useStore } from 'vuex';

	const CONFIRM_WORD = 'delete';

	export default defineComponent({
		name: 'PagesTrash',
		components: {
			AlertDialog,
			AlertDialogCancel,
			AlertDialogContent,
			AlertDialogDescription,
			AlertDialogFooter,
			AlertDialogHeader,
			AlertDialogTitle,
			Archive,
			AuthorBadge,
			Button,
			PageContainer,
			PageHeader,
			Trash2,
			Undo2,
		},
		setup() {
			const route = useRoute();
			const router = useRouter();
			const store = useStore();
			const toaster = useToast();
			const workspaceCode = computed(() => String(route.params.workspace_code));
			const pages = ref<TrashedPage[]>([]);
			const loaded = ref(false);
			const failed = ref(false);
			const restoring = ref<number | null>(null);
			const purging = ref<TrashedPage | null>(null);
			const emptyOpen = ref(false);
			const confirmWord = ref('');
			const busy = ref(false);

			const load = async () => {
				try {
					pages.value = await getPagesTrash();
					failed.value = false;
				} catch {
					failed.value = true;
				} finally {
					loaded.value = true;
				}
			};

			onMounted(load);
			watch(() => store.state.pagesEvent?.seq, load);

			const restore = async (page: TrashedPage) => {
				restoring.value = page.id;
				try {
					const restored = await restorePage(page.id);
					router.push(pageUrl(workspaceCode.value, restored.slug));
				} catch {
					toaster.toast({
						title: t('pages.trash.restoreFailed'),
						variant: 'destructive',
					});
				} finally {
					restoring.value = null;
				}
			};

			const fail = (title: string) =>
				toaster.toast({ title, variant: 'destructive' });

			const confirmPurge = async () => {
				const page = purging.value;
				if (!page || busy.value) return;
				busy.value = true;
				try {
					await permanentlyDeletePage(page.id);
					toaster.toast({ title: t('pages.trash.purged') });
				} catch (error: any) {
					const status = error?.response?.status;
					if (status === 403) fail(t('pages.trash.noPermissionDelete'));
					else if (status === 404)
						toaster.toast({ title: t('pages.trash.alreadyDeleted') });
					else if (status === 422)
						toaster.toast({ title: t('pages.trash.notArchived') });
					else fail(t('pages.trash.deleteFailed'));
					await load();
				} finally {
					busy.value = false;
					purging.value = null;
				}
			};

			const openEmpty = () => {
				confirmWord.value = '';
				emptyOpen.value = true;
			};

			const confirmed = computed(
				() => confirmWord.value.trim().toLowerCase() === CONFIRM_WORD,
			);

			const confirmEmpty = async () => {
				if (!confirmed.value || busy.value) return;
				busy.value = true;
				try {
					const { skipped } = await emptyPagesTrash();
					if (skipped > 0) {
						toaster.toast({
							title: t('pages.trash.someKept'),
							description: t(
								'pages.trash.someKeptBody',
								{ n: skipped },
								skipped,
							),
						});
					} else {
						toaster.toast({ title: t('pages.trash.emptied') });
					}
				} catch (error: any) {
					if (error?.response?.status === 403)
						fail(t('pages.trash.noPermissionEmpty'));
					else fail(t('pages.trash.emptyFailed'));
					await load();
				} finally {
					busy.value = false;
					emptyOpen.value = false;
				}
			};

			const countLabel = computed(() =>
				t(
					'pages.trash.countLabel',
					{ n: pages.value.length },
					pages.value.length,
				),
			);

			const formatDeleted = (value?: string | null) =>
				value ? new Date(value).toLocaleString(currentLocale()) : '';

			return {
				CONFIRM_WORD,
				workspaceCode,
				pages,
				loaded,
				failed,
				restoring,
				restore,
				purging,
				confirmPurge,
				emptyOpen,
				openEmpty,
				confirmWord,
				confirmed,
				confirmEmpty,
				countLabel,
				busy,
				formatDeleted,
				toAuthorRef,
			};
		},
	});
</script>
