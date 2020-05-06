import AuthBase from '../AuthBase'

export default {
    name: 'Login',
    components: {
        AuthBase
    },
    props: [],
    data() {
        return {
            email: null,
            password: null,
            errors: {}
        }
    },
    computed: {

    },
    mounted() {

    },
    methods: {
        login() {
            this.$axios.post('auth/login', {
                email: this.email,
                password: this.password
            })
                .then(({ data }) => {
                    this.$store.commit('token', data)
                    this.setUser()
                })
                .catch((error) => {
                    if (!error || !error.response) {
                        return
                    }
                    this.errors = error.response.data.errors
                })
        },
        setUser() {
            this.$axios.defaults.headers = {
                Authorization: `Bearer ${this.$store.getters.token.token}`,
                'X-Requested-With': 'XMLHttpRequest'
            }
            this.$axios.get('user').then(({ data }) => {
                this.$store.commit('user', data)
                this.$router.push({ name: 'CurrentTasksList' })
            })
        }
    }
}


