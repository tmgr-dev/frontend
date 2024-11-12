<template>
	<div class="mr-auto p-4 md:hidden">
		<svg
			ref="svgElement"
			xmlns="http://www.w3.org/2000/svg"
			width="40"
			height="38"
			viewBox="0 0 26 13"
		>
			<g fill="none" fill-rule="evenodd" transform="translate(5 2)">
				<rect
					width="9"
					height="1"
					x="7"
					y="1"
					fill="#4B545B"
					opacity=".4"
					rx=".5"
				/>
				<rect width="2" height="1" y="1" fill="#4B545B" opacity=".4" rx=".5" />
				<rect width="8" height="1" y="6" fill="#4B545B" opacity=".4" rx=".5" />
				<rect
					width="3"
					height="1"
					x="13"
					y="6"
					fill="#4B545B"
					opacity=".4"
					rx=".5"
				/>
				<rect
					width="7"
					height="1"
					x="9"
					y="11"
					fill="#4B545B"
					opacity=".4"
					rx=".5"
				/>
				<rect width="4" height="1" y="11" fill="#4B545B" opacity=".4" rx=".5" />
				<circle cx="4.5" cy="1.5" r="1.5" fill="#59BC7B" />
				<circle cx="10.5" cy="6.5" fill="#59BC7B" r="1.5" />
				<circle cx="6.5" cy="11.5" r="1.5" fill="#59BC7B" />
			</g>
		</svg>
	</div>
</template>

<script setup lang="ts">
	import { onMounted, Ref, ref, watch } from 'vue';

	interface Props {
		isActive: Boolean;
	}

	const props = defineProps<Props>();
	const svgElement: Ref<SVGSVGElement | null> = ref(null);

	onMounted(() => {
		offFilter();
	});

	watch(
		() => props.isActive,
		(isActive) => {
			if (isActive) {
				onFilter();
			} else {
				offFilter();
			}
		},
	);

	const offFilter = () => {
		if (svgElement.value) {
			const circles = svgElement.value.querySelectorAll('circle');
			circles.forEach((circle) => {
				circle.setAttribute('cx', '12');
			});
			const rectangles = svgElement.value.querySelectorAll('rect');
			rectangles[1].setAttribute('width', '10');
			rectangles[0].setAttribute('width', '2');
			rectangles[0].setAttribute('x', '14');

			rectangles[2].setAttribute('width', '10');
			rectangles[3].setAttribute('width', '2');
			rectangles[3].setAttribute('x', '14');

			rectangles[5].setAttribute('width', '10');
			rectangles[4].setAttribute('width', '2');
			rectangles[4].setAttribute('x', '14');
		}
	};
	const onFilter = () => {
		if (svgElement.value) {
			const circles = svgElement.value.querySelectorAll('circle');
			circles[0].setAttribute('cx', '4.5');
			circles[1].setAttribute('cx', '10.5');
			circles[2].setAttribute('cx', '6.5');

			const rectangles = svgElement.value.querySelectorAll('rect');

			rectangles[1].setAttribute('width', '2');
			rectangles[0].setAttribute('width', '9');
			rectangles[0].setAttribute('x', '7');

			rectangles[2].setAttribute('width', '8');
			rectangles[3].setAttribute('width', '3');
			rectangles[3].setAttribute('x', '13');

			rectangles[5].setAttribute('width', '4');
			rectangles[4].setAttribute('width', '7');
			rectangles[4].setAttribute('x', '9');
		}
	};
</script>

<style scoped>
	circle,
	rect {
		transition: all 0.2s ease;
	}
</style>
