import Login from '../components/Auth/Login'
import Register from '../components/Auth/Register'
import Index from '../components/Index'
import CategoryForm from '../components/Categories/CategoryForm'
import CategoryList from '../components/Categories/CategoryList'
import CategoryView from '../components/Categories/CategoryView'

const routes = [
    {
        path: '/',
        component: Index
    },{
        path: '/register',
        component: Register
    },{
        path: '/login',
        component: Login
    },{
        path: '/categories',
        component: CategoryList
    },{
        path: '/categories/view',
        component: CategoryView
    },{
        path: '/categories/form',
        component: CategoryForm
    },
]

export default routes
