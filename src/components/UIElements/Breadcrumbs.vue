<template>
	<div
		class="card transition duration-300 ease-in-out hover:shadow-sm flex flex-col my-5"
	>
		<div class="card-body py-4">
			<nav aria-label="breadcrumb" class="text-left">
				<ol v-if="current" class="breadcrumb md:flex">
					<li class="inline md:hidden">Breadcrumbs:</li>

					<li
						v-for="(item, i) in items"
						:key="i"
						class="breadcrumb-item inline text-gray-600"
						@dragleave="item.payload.hoverClass = ''"
						@drop="drop($event, item.payload)"
						@dragenter.prevent="item.payload.hoverClass = 'bg-red-500'"
						@dragover.prevent="item.payload.hoverClass = 'bg-red-500'"
					>
						<router-link
							:to="item.to"
							class="text-gray-300 hover:text-gray-100 mx-0 md:mx-1"
						>
							{{ item.label }}
						</router-link>
					</li>
					<li
						aria-current="page"
						class="breadcrumb-item active inline text-gray-300 font-bold md:mx-1"
					>
						{{ current }}
					</li>
				</ol>

				<ol v-else class="breadcrumb flex pl-0">
					<li aria-current="page" class="breadcrumb-item active text-gray-300">
						Categories
					</li>
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

	@media screen and (max-width: 768px) {
		.breadcrumb .breadcrumb-item:first-child:before {
			content: '->';
		}
	}
</style>
