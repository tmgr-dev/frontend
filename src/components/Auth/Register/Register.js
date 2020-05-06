import AuthBase from '../AuthBase'

export default {
  name: 'Register',
  components: {
      AuthBase
  },
  props: [],
  data () {
    return {
        name: null,
        email: null,
        password: null,
        password_confirmation: null,
        errors: {}
    }
  },
  computed: {

  },
  mounted () {

  },
  methods: {
    register () {
        this.$axios.post('auth/register', {
            name: this.name,
            email: this.email,
            password: this.password,
            password_confirmation: this.password_confirmation
        })
            .then(({data: { data }}) => {
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
    setUser () {
        this.$axios.defaults.headers = {
            Authorization: `Bearer ${this.$store.getters.token.token}`,
            'X-Requested-With': 'XMLHttpRequest'
        }
        this.$axios.get('user').then(({data}) => {
            this.$store.commit('user', data)
            this.$router.push({name: 'CurrentTasksList'})
        })
    }
  }
}


