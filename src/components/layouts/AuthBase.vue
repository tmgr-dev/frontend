<template>
	<section
		class="flex h-screen w-screen items-center justify-center overflow-hidden text-neutral-600"
	>
		<div class="h-full w-full bg-blue-400">
			<div
				class="flex h-full flex-1 flex-col items-center justify-center px-4 sm:px-0"
			>
				<div
					class="flex min-h-[500px] w-full rounded-lg bg-white shadow-lg sm:mx-0 sm:w-3/4 lg:w-1/2"
				>
					<div class="flex w-full flex-col p-4 md:w-1/2">
						<div class="mb-8 flex flex-1 flex-col justify-center">
							<h1 class="text-center text-4xl font-thin">
								<slot name="title" />
							</h1>

							<div class="mt-4 w-full">
								<slot name="body" />

								<div class="mt-4 text-center">
									<slot name="footer" />
								</div>
							</div>
						</div>
					</div>

					<div
						class="hidden rounded-r-lg bg-[url(../../assets/img/bg-login.jpeg)] bg-cover bg-center bg-no-repeat md:block md:w-1/2"
					/>
				</div>
			</div>
		</div>
	</section>
</template>
