<template>
	<ul
		:class="`flex flex-col mt-4 -mx-4 pt-4 md:flex-row md:items-center md:mx-0 md:mt-0 md:pt-0 md:mr-4 lg:mr-8 md:border-0`"

	>
		<router-link
			v-for="link in links"
			:key="link.id"
			:to="link.path"
			custom
			v-slot="{ href, isActive, navigate }">
			<li class="tc-block px-4 py-1 md:p-2 lg:px-4" :class="isActive ? activeLinkClass : ''">
				<a :href="href" @click="navigate">
					{{ link.name }} <span class="top-0 text-red-500 relative text-sm" style="top: -10px;" v-html="link.small"></span>
				</a>
			</li>
		</router-link>
	</ul>
</template>

<script>
	export default {
		name: 'NavbarMenu',
		data: () => ({
			links: [
				{ id: 1, name: 'Current tasks', path: '/' },
				{ id: 1, name: 'Hidden', path: '/hidden' },
				{ id: 1, name: 'Board', small: 'in dev', path: '/board' },
				// { id: 1, name: 'Archive', path: '/archive' },
				{ id: 1, name: 'Categories', path: '/projects-categories' }
			]
		}),
		computed: {
			activeLinkClass () {
				return `${this.$color('navLinkFocused')}-600`
			}
		}
	}
</script>
