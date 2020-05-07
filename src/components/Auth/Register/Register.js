import AuthBase from '../AuthBase'

export default {
    name: 'Register',
    components: {
        AuthBase
    },
    props: [],
    data() {
        return {
            showLoader: false,
            form: {
                name: null,
                email: null,
                password: null,
                password_confirmation: null,
            },
            errors: {}
        }
    },
    methods: {
        async register() {
            const {...registerData} = this.form
            try {
                this.showLoader = true
                const { data } = await this.$axios.post('auth/register', registerData)
                this.showLoader = false
                
                this.$store.commit('token', data.data)
                this.setUser()
                
            } catch (error) {
                this.showLoader = false

                if (error && error.response) {
                    this.errors = error.response.data.errors
                }
            }
        },

        async setUser() {
            this.$axios.defaults.headers = {
                Authorization: `Bearer ${this.$store.getters.token.token}`,
                'X-Requested-With': 'XMLHttpRequest'
            }
            const { data } = await this.$axios.get('user')

            this.$store.commit('user', data)
            this.$router.push({ name: 'CurrentTasksList' })
        }
    }
}


