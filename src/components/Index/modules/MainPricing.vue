<template>
	<section class="bg-gray-100 py-8">
		<div class="container mx-auto px-2 pt-4 pb-12 text-gray-800">
			<h1
				class="my-2 w-full text-center text-5xl font-bold leading-tight text-gray-800"
			>
				Pricing
			</h1>
			<div class="mb-4 w-full">
				<div class="gradient mx-auto my-0 h-1 w-64 rounded-t py-0 opacity-25" />
			</div>
			<div class="my-12 flex flex-col justify-center pt-12 sm:my-4 sm:flex-row">
				<div
					class="z-10 mx-auto mt-4 flex w-5/6 flex-col rounded-lg bg-white shadow-lg sm:-mt-6 lg:mx-0 lg:w-1/3"
				>
					<div
						class="flex-1 overflow-hidden rounded-t rounded-b-none bg-white shadow"
					>
						<div class="w-full p-8 text-center text-3xl font-bold">Free</div>
						<div class="gradient my-0 h-1 w-full rounded-t py-0" />
						<ul class="w-full text-center text-base font-bold">
							<li class="border-b py-4">Manage your tasks</li>
							<li class="border-b py-4">Create cateogiries</li>
							<li class="border-b py-4">Run time counter</li>
						</ul>
					</div>
					<div
						class="mt-auto flex-none overflow-hidden rounded-b rounded-t-none bg-white p-6 shadow"
					>
						<div class="w-full pt-6 text-center text-4xl font-bold">
							£0.00 <span class="text-base">/ per user</span>
						</div>
						<div class="flex items-center justify-center">
							<router-link
								class="gradient mx-auto my-6 rounded-full py-4 px-8 font-bold text-black shadow-lg hover:underline lg:mx-0"
								to="/register"
							>
								Sign Up
							</router-link>
						</div>
					</div>
				</div>
			</div>
		</div>
	</section>
</template>

<script>
	export default {
		name: 'MainPricing',
	};
</script>
