<template>
	<span class="relative flex h-9 w-9 cursor-pointer">
		<span
			class="absolute h-full w-full animate-ping rounded-full bg-status-done opacity-30"
		/>
		<span
			class="relative flex h-full w-full items-center justify-center rounded-full bg-status-done text-sm font-bold text-white"
		>
			<slot />
		</span>
	</span>
</template>
