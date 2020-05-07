<template>
    <div class="mini_loader_wrapper" :class="{active: isActive}" :style="sidePositionStyle">
        <div class="mini_loader">
            <div></div>
            <div></div>
            <div></div>
            <div></div>
        </div>
    </div>
</template>

<script>
    export default {
        name: "MiniLoader",
        props: {
            sidePositionStyle: {
                type: String,
                required: false,
                default: 'left: 100%'
            },
            isActive: {
                type: Boolean,
                required: true,
                default: true
            }
        },
    };
</script>

<style lang="scss" scoped>
    .mini_loader_wrapper {
        position: absolute;
        left: auto;
        right: auto;
        margin-left: 10px;
        margin-right: 10px;
        top: calc(50% - 7px);
        visibility: hidden;
        opacity: 0;
        transition: opacity .2s;
        &.active {
            opacity: 1;
            visibility: visible;
        }
    }
    .mini_loader {
        position: relative;
        width: 20px;
        height: 20px;
        div {
            box-sizing: border-box;
            display: block;
            position: absolute;
            width: 12px;
            height: 12px;
            border: 2px solid #fff;
            border-radius: 50%;
            animation: lds-ring 1.2s cubic-bezier(0.5, 0, 0.5, 1) infinite;
            border-color: #fff transparent transparent transparent;
        }

        div:nth-child(1) {
            animation-delay: -0.45s;
        }

        div:nth-child(2) {
            animation-delay: -0.3s;
        }

        div:nth-child(3) {
            animation-delay: -0.15s;
        }
    }

    @keyframes lds-ring {
        0% {
            transform: rotate(0deg);
        }
        100% {
            transform: rotate(360deg);
        }
    }
</style>
