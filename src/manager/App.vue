<template>
	<div class="manager app-shell">
		<aside class="side">
			<div class="top">
				<b>{{ $t('manager.list.label') }}</b>
				<button
					type="button"
					class="ibd"
					:aria-label="$t('manager.trust.devices.open')"
					:title="$t('manager.trust.devices.open')"
					@click="trust.openPanel()"
				>
					<Icon name="gear" />
				</button>
			</div>
			<button type="button" class="sidesrch" @click="store.openSearch()">
				<span>{{ $t('manager.search.scopeAll') }}</span>
				<kbd>&#8984;K</kbd>
			</button>
			<ChatList
				:rooms="store.rooms"
				:active-key="store.activeKey"
				:unread="store.unreadByRoom"
				@select="store.select"
			/>
		</aside>
		<section class="main">
			<ChatHeader
				v-if="activeInfo"
				:info="activeInfo"
				@search="store.openSearch(store.activeKey ?? undefined)"
			/>
			<SearchPanel
				v-if="store.search.open"
				:query="store.search.text"
				:scope="store.search.scopeKey ? 'chat' : 'all'"
				:chat-label="activeInfo?.title ?? ''"
				:hits="store.search.hits"
				:room-titles="roomTitles"
				@update:query="store.runSearch"
				@update:scope="onScope"
				@select="onHit"
			/>
			<template v-else>
				<MessageList
					ref="list"
					:messages="messages"
					:body-html="bodyHtml"
					@reply="store.setReply"
					@copy="copy"
					@older="store.loadOlder"
				>
					<template #decision="{ message }">
						<div class="decision-ref">
							{{ message.decision?.card_id }}
						</div>
					</template>
				</MessageList>
				<Composer
					v-model="store.draft.text"
					:reply-to="store.replyTo ?? undefined"
					:attachments="store.draft.attachments"
					@update:model-value="store.onInput"
					@send="store.send"
					@clear-reply="store.setReply(null)"
					@remove-attachment="store.removeAttachment"
					@attach="pick"
					@drop="addFiles"
					@keydown="onKey"
				>
					<template #popover
						><SlashMenu
							v-if="store.slash.open"
							:commands="store.slash.items"
							:active-index="store.slash.index"
							@select="store.pickSlash"
					/></template>
				</Composer>
			</template>
			<p v-if="store.error" class="mgr-error" role="alert">
				{{ store.error }}
			</p>
		</section>
		<input ref="file" type="file" multiple hidden @change="onFiles" />
		<JoinConfirmDialog
			v-if="trust.current?.type === 'join'"
			:label="trust.current.join.label"
			:kind="trust.current.join.kind"
			:fp="trust.current.join.fp"
			:busy="trust.busy"
			:error="trust.error"
			@approve="trust.decideJoin(true)"
			@reject="trust.decideJoin(false)"
		/>
		<RevokeConfirmDialog
			v-else-if="trust.current?.type === 'revoke'"
			:revoker-label="trust.current.revoke.revoker_label"
			:revoker-fp="trust.current.revoke.revoker_fp"
			:target-label="trust.current.revoke.target_label"
			:target-fp="trust.current.revoke.target_fp"
			:busy="trust.busy"
			:error="trust.error"
			@approve="trust.decideRevoke(true)"
			@reject="trust.decideRevoke(false)"
		/>
		<RevokeConfirmDialog
			v-else-if="revokeDevice"
			:target-label="revokeDevice.label"
			:target-fp="revokeDevice.fp"
			:busy="trust.busy"
			:error="trust.error"
			@approve="trust.confirmRevoke()"
			@reject="trust.cancelRevoke()"
		/>
		<ResetIdentityDialog
			v-else-if="trust.resetOpen"
			v-model="trust.resetText"
			:busy="trust.busy"
			:error="trust.error"
			@confirm="trust.resetIdentity()"
			@cancel="trust.cancelReset()"
		/>
		<DevicesPanel
			v-else-if="trust.panelOpen"
			:devices="trust.trust?.devices ?? []"
			:busy="trust.busy"
			:error="trust.error"
			@revoke="trust.revoke"
			@reset="trust.openReset()"
			@close="trust.closePanel()"
		/>
	</div>
</template>

<script lang="ts">
	import { defineComponent } from 'vue';
	import ChatHeader from './components/ChatHeader.vue';
	import ChatList from './components/ChatList.vue';
	import Composer from './components/Composer.vue';
	import Icon from './components/Icon.vue';
	import MessageList from './components/MessageList.vue';
	import SearchPanel from './components/SearchPanel.vue';
	import SlashMenu from './components/SlashMenu.vue';
	import DevicesPanel from './components/trust/DevicesPanel.vue';
	import JoinConfirmDialog from './components/trust/JoinConfirmDialog.vue';
	import ResetIdentityDialog from './components/trust/ResetIdentityDialog.vue';
	import RevokeConfirmDialog from './components/trust/RevokeConfirmDialog.vue';
	import { renderBody } from './format';
	import { MAX_FILE_BYTES, useManagerStore } from './store';
	import { useTrustStore } from './trustStore';
	import { roomKey, type ChatMessage } from './types';

	const sha256Hex = async (file: File): Promise<string> => {
		const digest = await crypto.subtle.digest(
			'SHA-256',
			await file.arrayBuffer(),
		);
		return Array.from(new Uint8Array(digest), (b) =>
			b.toString(16).padStart(2, '0'),
		).join('');
	};

	export default defineComponent({
		name: 'ManagerApp',
		components: {
			ChatHeader,
			ChatList,
			Composer,
			DevicesPanel,
			Icon,
			JoinConfirmDialog,
			MessageList,
			ResetIdentityDialog,
			RevokeConfirmDialog,
			SearchPanel,
			SlashMenu,
		},
		setup() {
			return { store: useManagerStore(), trust: useTrustStore() };
		},
		computed: {
			revokeDevice() {
				return (
					this.trust.trust?.devices.find(
						(d) => d.id === this.trust.revokeTarget,
					) ?? null
				);
			},
			activeInfo() {
				return this.store.rooms.find(
					(r) => roomKey(r.room) === this.store.activeKey,
				);
			},
			messages() {
				return this.store.activeKey
					? (this.store.messagesByRoom[this.store.activeKey] ?? [])
					: [];
			},
			bodyHtml(): Record<string, string> {
				return Object.fromEntries(
					this.messages.map((m) => [m.id, renderBody(m.body_md)]),
				);
			},
			roomTitles(): Record<string, string> {
				return Object.fromEntries(
					this.store.rooms.map((r) => [roomKey(r.room), r.title]),
				);
			},
		},
		mounted() {
			void this.store.init();
			void this.trust.init();
			window.addEventListener('keydown', this.onGlobalKey);
		},
		beforeUnmount() {
			window.removeEventListener('keydown', this.onGlobalKey);
			this.trust.dispose();
		},
		methods: {
			onGlobalKey(e: KeyboardEvent) {
				if ((e.metaKey || e.ctrlKey) && e.key === 'k') {
					e.preventDefault();
					this.store.openSearch();
				} else if (e.key === 'Escape' && this.store.search.open) {
					this.store.closeSearch();
				}
			},
			onKey(e: KeyboardEvent) {
				if (!this.store.slash.open) return;
				if (e.key === 'ArrowDown') {
					e.preventDefault();
					this.store.moveSlash(1);
				} else if (e.key === 'ArrowUp') {
					e.preventDefault();
					this.store.moveSlash(-1);
				} else if (e.key === 'Tab' || e.key === 'Enter') {
					e.preventDefault();
					this.store.pickSlash();
				} else if (e.key === 'Escape') {
					this.store.closeSlash();
				}
			},
			onScope(scope: 'chat' | 'all') {
				this.store.search.scopeKey =
					scope === 'chat' ? this.store.activeKey : null;
				void this.store.runSearch(this.store.search.text);
			},
			async onHit(hit: Parameters<typeof this.store.jumpToHit>[0]) {
				this.store.closeSearch();
				await this.store.jumpToHit(hit);
			},
			copy(text: string) {
				void navigator.clipboard?.writeText(text);
			},
			pick() {
				(this.$refs.file as HTMLInputElement).click();
			},
			onFiles(e: Event) {
				const input = e.target as HTMLInputElement;
				void this.addFiles(Array.from(input.files ?? []));
				input.value = '';
			},
			async addFiles(files: File[]) {
				for (const file of files) {
					this.store.addAttachment({
						name: file.name,
						type: file.type || 'application/octet-stream',
						size: file.size,
						sha256: await sha256Hex(file),
					});
				}
			},
		},
	});
</script>

<style src="./manager.css"></style>
