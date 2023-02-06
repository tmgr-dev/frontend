<template>
	<ul
		class="flex flex-col mt-4 gap-2 md:gap-7 pt-4 md:flex-row md:items-center md:mx-0 md:mt-0 md:pt-0 md:mr-4 lg:mr-8 md:border-0"
	>
		<router-link
			v-for="link in links"
			:key="link.id"
			v-slot="{ href, isActive, navigate }"
			:to="link.path"
			custom
		>
			<li :class="{ 'text-purple dark:text-green': isActive }">
				<a :href="href" @click="navigate">
					{{ link.name }}
					<span
						class="top-0 text-red-500 relative text-sm"
						v-html="link.small"
					/>
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
				{ id: 1, name: 'List', path: '/' },
				{ id: 2, name: 'Board', path: '/board' },
				{ id: 3, name: 'Categories', path: '/projects-categories' },
			],
		}),
	};
</script>
