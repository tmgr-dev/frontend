# [SPA Task Manager](http://tmgr.dev/)  

Lightweight task manager application.

## Project setup
```
npm install
```

### Compiles and hot-reloads for development
```
npm run serve
```

### Compiles and minifies for production
```
npm run build
```

### Lints and fixes files
```
npm run lint
```

### Customize configuration
See [Configuration Reference](https://cli.vuejs.org/config/).

### Creating new component
For creating new component use [vue-generate-component](https://www.npmjs.com/package/vue-generate-component)

Note: component will be created in current folder

### Git Flow

Branch name for production releases: `master`

Branch name for "next release" development: `develop` 

How to name your supporting branch prefixes?

Feature branches: `feature`

Bugfix branches: `bugfix`

Release branches:`release`

Hotfix branches: `hotfix`

Support branches: `support`

Version tag prefix: `v`

Hooks and filters directory: `./.git/hooks` 

### Environment variables

First of all you have to create .env file

    cp example.env .env
    
#### Variables

`VUE_APP_API_BASE_URL` - API base URL

    # Example:
    VUE_APP_API_BASE_URL=http://taskmanager.localhost/api/


# Desktop Version in development. Soon...
