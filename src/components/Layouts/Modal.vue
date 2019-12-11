<template>
    <div class="overlay" :class="{ active: show }" @click="close">
        <div class="modal">
            <slot name="modal-body"></slot>
        </div>
    </div>
</template>

<script>
export default {
    name: 'Modal',
    props: {
        show: {
            type: Boolean,
            required: false,
            default: () => false
        }
    },
    methods: {
        close(e) {
            if (e.target.classList.contains('overlay')) {                
                this.$emit('close')
            }
        }
    },
}
</script>

<style lang="scss">
    .overlay {
        position: fixed;
        top: 0;
        right: 0;
        width: 100%;
        height: 100%;
        background: rgba(0, 0, 0, 0.555);
        z-index: -1;
        opacity: 0;
        transition: .4s;
        transform: translateX(100%);
        &.active {
            z-index: 100;
            opacity: 1;
            transform: translateX(0);
        }

    }
    .modal {
        background: #fff;
        max-width: 80%;
        margin: 50px auto;
        padding: 30px;
        nav {
            display: none;
        }
    }    
</style>