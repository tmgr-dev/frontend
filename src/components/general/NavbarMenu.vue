<template>
	<ul
		class="mt-4 flex flex-col gap-2 pt-4 md:mx-0 md:mt-0 md:mr-4 md:flex-row md:items-center md:gap-7 md:border-0 md:pt-0 lg:mr-8"
	>
		<router-link
			v-for="link in links"
			:key="link.id"
			v-slot="{ href, isActive, navigate }"
			:to="link.path"
			custom
		>
			<li :class="{ 'text-purple dark:text-green': isActive }">
				<a
					:href="href"
					:class="{ 'font-bold': isActive }"
					@click="
						() => {
							navigate();
							$emit('navigated');
						}
					"
				>
					{{ link.name }}
				</a>
			</li>
		</router-link>
	</ul>
</template>

<script lang="ts" setup>
	defineEmits(['navigated']);

	const links = [
		{ id: 1, name: 'List', path: '/' },
		{ id: 2, name: 'Board', path: '/board' },
		{ id: 3, name: 'Categories', path: '/projects-categories' },
	];
</script>
