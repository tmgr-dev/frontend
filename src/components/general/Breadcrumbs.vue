<template>
	<div class="card flex flex-col transition duration-300 ease-in-out">
		<div class="card-body py-4">
			<nav aria-label="breadcrumb" class="text-left">
				<ol v-if="current" class="breadcrumb md:flex">
					<li class="inline md:hidden">Breadcrumbs:</li>

					<li
						v-for="(item, i) in items"
						:key="i"
						class="breadcrumb-item inline text-neutral-600 dark:text-neutral-400"
						@dragleave="item.payload.hoverClass = ''"
						@drop="drop($event, item.payload)"
						@dragenter.prevent="item.payload.hoverClass = 'bg-red-500'"
						@dragover.prevent="item.payload.hoverClass = 'bg-red-500'"
					>
						<router-link :to="item.to" class="mx-0 md:mx-1">
							{{ item.label }}
						</router-link>
					</li>
					<li
						aria-current="page"
						class="breadcrumb-item active inline font-bold md:mx-1"
					>
						{{ current }}
					</li>
				</ol>

				<ol v-else class="breadcrumb flex pl-0">
					<li aria-current="page" class="breadcrumb-item">Categories</li>
				</ol>
			</nav>
		</div>
	</div>
</template>

<script>
	export default {
		name: 'Breadcrumbs',
		props: {
			items: {
				required: true,
				type: Array,
			},
			title: {
				required: false,
				type: String,
				default: 'Breadcrumbs',
			},
			current: {
				required: false,
				type: String,
				default: 'Current page',
			},
			drop: {
				type: Function,
				required: false,
				default: () => () => {},
			},
		},
	};
</script>

<style lang="scss" scoped>
	.breadcrumb .breadcrumb-item {
		position: relative;
	}

	.breadcrumb .breadcrumb-item:not(:last-child):after {
		content: '/';

		@media screen and (max-width: 768px) {
			content: '/';
		}
	}
</style>
