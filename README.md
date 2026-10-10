# [SPA Task Manager](http://tmgr.dev/)

Lightweight task manager application.

## Install on NixOS

The Linux desktop app is packaged as a flake that wraps the release `.deb`. It pins the latest desktop release and is bumped automatically after each release.

```
nix profile install github:tmgr-dev/frontend#tmgr
nix run github:tmgr-dev/frontend#tmgr
```

NixOS configuration:

```nix
{
  inputs.tmgr.url = "github:tmgr-dev/frontend";

  outputs = { nixpkgs, tmgr, ... }: {
    nixosConfigurations.host = nixpkgs.lib.nixosSystem {
      system = "x86_64-linux";
      modules = [
        { environment.systemPackages = [ tmgr.packages.x86_64-linux.default ]; }
      ];
    };
  };
}
```

The built-in updater is disabled in this package; update with `nix flake update tmgr`.

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

`VITE_API_BASE_URL` - API base URL

    # Example:
    VITE_API_BASE_URL=http://taskmanager.localhost/api/

# TO BE CONTINUED...
