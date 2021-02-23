<template>
	<ul class="flex flex-col mt-4 -mx-4 pt-4 border-t md:flex-row md:items-center md:mx-0 md:mt-0 md:pt-0 md:mr-4 lg:mr-8 md:border-0">
		<router-link
			v-for="link in links"
			:key="link.id"
			:to="link.path"
			class="block px-4 py-1 md:p-2 lg:px-4"
			:active-class="`${$color('navLinkFocused')}-600`"
			custom
			v-slot="{ href, isActive, navigate }">
			<a :href="href" @click="navigate">
				{{ link.name }}
			</a>
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
				{ id: 1, name: 'Archive', path: '/acrhive' },
				{ id: 1, name: 'Categories', path: '/projects-categories' },
			]
		})
	}
</script>
