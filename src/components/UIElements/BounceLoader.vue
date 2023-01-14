<template>
	<div>
		<div class="bounce-ball"></div>
	</div>
</template>

<style lang="scss">
$width: 15px;
$height: 15px;
$bounce_height: 15px;

.bounce-ball {
	position: relative;
	display: inline-block;
	height: 37px;
	width: $width;

	&:before {
		position: absolute;
		content: '';
		display: block;
		top: 0;
		width: $width;
		height: $height;
		border-radius: 50%;
		background-color: #fbae17;
		transform-origin: 50%;
		animation: bounce 500ms alternate infinite ease;
	}
}

@keyframes bounce {
	0% {
		top: $bounce_height;
		height: 5px;
		border-radius: 60px 60px 20px 20px;
		transform: scaleX(2);
	}
	35% {
		height: $height;
		border-radius: 50%;
		transform: scaleX(1);
	}
	100% {
		top: 0;
	}
}
</style>
