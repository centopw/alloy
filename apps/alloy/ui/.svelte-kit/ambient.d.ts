
// this file is generated — do not edit it


/// <reference types="@sveltejs/kit" />

/**
 * This module provides access to environment variables that are injected _statically_ into your bundle at build time and are limited to _private_ access.
 * 
 * |         | Runtime                                                                    | Build time                                                               |
 * | ------- | -------------------------------------------------------------------------- | ------------------------------------------------------------------------ |
 * | Private | [`$env/dynamic/private`](https://svelte.dev/docs/kit/$env-dynamic-private) | [`$env/static/private`](https://svelte.dev/docs/kit/$env-static-private) |
 * | Public  | [`$env/dynamic/public`](https://svelte.dev/docs/kit/$env-dynamic-public)   | [`$env/static/public`](https://svelte.dev/docs/kit/$env-static-public)   |
 * 
 * Static environment variables are [loaded by Vite](https://vitejs.dev/guide/env-and-mode.html#env-files) from `.env` files and `process.env` at build time and then statically injected into your bundle at build time, enabling optimisations like dead code elimination.
 * 
 * **_Private_ access:**
 * 
 * - This module cannot be imported into client-side code
 * - This module only includes variables that _do not_ begin with [`config.kit.env.publicPrefix`](https://svelte.dev/docs/kit/configuration#env) _and do_ start with [`config.kit.env.privatePrefix`](https://svelte.dev/docs/kit/configuration#env) (if configured)
 * 
 * For example, given the following build time environment:
 * 
 * ```env
 * ENVIRONMENT=production
 * PUBLIC_BASE_URL=http://site.com
 * ```
 * 
 * With the default `publicPrefix` and `privatePrefix`:
 * 
 * ```ts
 * import { ENVIRONMENT, PUBLIC_BASE_URL } from '$env/static/private';
 * 
 * console.log(ENVIRONMENT); // => "production"
 * console.log(PUBLIC_BASE_URL); // => throws error during build
 * ```
 * 
 * The above values will be the same _even if_ different values for `ENVIRONMENT` or `PUBLIC_BASE_URL` are set at runtime, as they are statically replaced in your code with their build time values.
 */
declare module '$env/static/private' {
	export const NODE_ENV: string;
	export const npm_node_execpath: string;
	export const COLORTERM: string;
	export const CLAUDECODE: string;
	export const OSLogRateLimit: string;
	export const INFOPATH: string;
	export const AGENT: string;
	export const npm_config_user_agent: string;
	export const BUN_INSTALL: string;
	export const DEBIAN_FRONTEND: string;
	export const COMPOSER_NO_INTERACTION: string;
	export const PIP_DISABLE_PIP_VERSION_CHECK: string;
	export const HERDR_PANE_ID: string;
	export const KUBECONFIG: string;
	export const ATUIN_SESSION: string;
	export const XDG_CACHE_HOME: string;
	export const PIP_NO_INPUT: string;
	export const GH_PAGER: string;
	export const MANPAGER: string;
	export const GPG_TTY: string;
	export const ATUIN_HISTORY_ID: string;
	export const STARSHIP_SHELL: string;
	export const npm_package_version: string;
	export const SSH_ASKPASS: string;
	export const COREPACK_ENABLE_DOWNLOAD_PROMPT: string;
	export const CI: string;
	export const HERDR_SOCKET_PATH: string;
	export const CURSOR_CLI_BLOCK_CURSOR_AGENT: string;
	export const TERM_PROGRAM_VERSION: string;
	export const ATUIN_TMUX_POPUP: string;
	export const NODE_PATH: string;
	export const npm_package_name: string;
	export const LANG: string;
	export const PAGER: string;
	export const ITERM_PROFILE: string;
	export const EDITOR: string;
	export const LESS: string;
	export const BAT_PAGER: string;
	export const JAVA_HOME: string;
	export const YARN_ENABLE_TELEMETRY: string;
	export const DELTA_PAGER: string;
	export const XPC_FLAGS: string;
	export const npm_command: string;
	export const GH_PROMPT_DISABLED: string;
	export const LOGNAME: string;
	export const GLAB_PAGER: string;
	export const STARSHIP_SESSION_KEY: string;
	export const npm_lifecycle_script: string;
	export const npm_package_json: string;
	export const TERMINFO_DIRS: string;
	export const TERM: string;
	export const npm_config_progress: string;
	export const SYSTEMD_PAGER: string;
	export const NO_COLOR: string;
	export const SSH_AUTH_SOCK: string;
	export const HERDR_TAB_ID: string;
	export const XPC_SERVICE_NAME: string;
	export const TERM_SESSION_ID: string;
	export const CLOUDSDK_CORE_DISABLE_PROMPTS: string;
	export const PATH: string;
	export const HOME: string;
	export const TERM_FEATURES: string;
	export const npm_config_update_notifier: string;
	export const ATUIN_SHLVL: string;
	export const npm_execpath: string;
	export const MANPATH: string;
	export const TF_INPUT: string;
	export const VISUAL: string;
	export const PNPM_UPDATE_NOTIFIER: string;
	export const __CF_USER_TEXT_ENCODING: string;
	export const OMPCODE: string;
	export const YARN_ENABLE_PROGRESS_BARS: string;
	export const HERDR_ENV: string;
	export const PSQL_PAGER: string;
	export const XDG_CONFIG_HOME: string;
	export const GIT_TERMINAL_PROMPT: string;
	export const SVELTEKIT_FORK: string;
	export const HOMEBREW_PAGER: string;
	export const SHLVL: string;
	export const HERDR_WORKSPACE_ID: string;
	export const PNPM_SCRIPT_SRC_DIR: string;
	export const COMMAND_MODE: string;
	export const PYENV_SHELL: string;
	export const PNPM_DISABLE_SELF_UPDATE_CHECK: string;
	export const __CFBundleIdentifier: string;
	export const HOMEBREW_CELLAR: string;
	export const GIT_EDITOR: string;
	export const USER: string;
	export const npm_config_yes: string;
	export const PNPM_HOME: string;
	export const LC_ALL: string;
	export const NVM_DIR: string;
	export const COREPACK_ROOT: string;
	export const CARGO_TERM_PROGRESS_WHEN: string;
	export const AWS_PAGER: string;
	export const TMPDIR: string;
	export const PYTHONUNBUFFERED: string;
	export const HOMEBREW_PREFIX: string;
	export const HERDR_BIN_PATH: string;
	export const GIT_PAGER: string;
	export const HOMEBREW_REPOSITORY: string;
	export const TOKEN: string;
	export const TF_IN_AUTOMATION: string;
	export const npm_config_node_gyp: string;
	export const PWD: string;
	export const NVM_CD_FLAGS: string;
	export const NODE: string;
	export const COLORFGBG: string;
	export const INIT_CWD: string;
	export const MYSQL_PAGER: string;
	export const PYENV_ROOT: string;
	export const FPATH: string;
	export const pnpm_config_verify_deps_before_run: string;
	export const XDG_DATA_HOME: string;
	export const npm_config_fund: string;
	export const TERM_PROGRAM: string;
	export const npm_config_audit: string;
	export const MARKPATH: string;
	export const npm_lifecycle_event: string;
}

/**
 * This module provides access to environment variables that are injected _statically_ into your bundle at build time and are _publicly_ accessible.
 * 
 * |         | Runtime                                                                    | Build time                                                               |
 * | ------- | -------------------------------------------------------------------------- | ------------------------------------------------------------------------ |
 * | Private | [`$env/dynamic/private`](https://svelte.dev/docs/kit/$env-dynamic-private) | [`$env/static/private`](https://svelte.dev/docs/kit/$env-static-private) |
 * | Public  | [`$env/dynamic/public`](https://svelte.dev/docs/kit/$env-dynamic-public)   | [`$env/static/public`](https://svelte.dev/docs/kit/$env-static-public)   |
 * 
 * Static environment variables are [loaded by Vite](https://vitejs.dev/guide/env-and-mode.html#env-files) from `.env` files and `process.env` at build time and then statically injected into your bundle at build time, enabling optimisations like dead code elimination.
 * 
 * **_Public_ access:**
 * 
 * - This module _can_ be imported into client-side code
 * - **Only** variables that begin with [`config.kit.env.publicPrefix`](https://svelte.dev/docs/kit/configuration#env) (which defaults to `PUBLIC_`) are included
 * 
 * For example, given the following build time environment:
 * 
 * ```env
 * ENVIRONMENT=production
 * PUBLIC_BASE_URL=http://site.com
 * ```
 * 
 * With the default `publicPrefix` and `privatePrefix`:
 * 
 * ```ts
 * import { ENVIRONMENT, PUBLIC_BASE_URL } from '$env/static/public';
 * 
 * console.log(ENVIRONMENT); // => throws error during build
 * console.log(PUBLIC_BASE_URL); // => "http://site.com"
 * ```
 * 
 * The above values will be the same _even if_ different values for `ENVIRONMENT` or `PUBLIC_BASE_URL` are set at runtime, as they are statically replaced in your code with their build time values.
 */
declare module '$env/static/public' {
	
}

/**
 * This module provides access to environment variables set _dynamically_ at runtime and that are limited to _private_ access.
 * 
 * |         | Runtime                                                                    | Build time                                                               |
 * | ------- | -------------------------------------------------------------------------- | ------------------------------------------------------------------------ |
 * | Private | [`$env/dynamic/private`](https://svelte.dev/docs/kit/$env-dynamic-private) | [`$env/static/private`](https://svelte.dev/docs/kit/$env-static-private) |
 * | Public  | [`$env/dynamic/public`](https://svelte.dev/docs/kit/$env-dynamic-public)   | [`$env/static/public`](https://svelte.dev/docs/kit/$env-static-public)   |
 * 
 * Dynamic environment variables are defined by the platform you're running on. For example if you're using [`adapter-node`](https://github.com/sveltejs/kit/tree/main/packages/adapter-node) (or running [`vite preview`](https://svelte.dev/docs/kit/cli)), this is equivalent to `process.env`.
 * 
 * **_Private_ access:**
 * 
 * - This module cannot be imported into client-side code
 * - This module includes variables that _do not_ begin with [`config.kit.env.publicPrefix`](https://svelte.dev/docs/kit/configuration#env) _and do_ start with [`config.kit.env.privatePrefix`](https://svelte.dev/docs/kit/configuration#env) (if configured)
 * 
 * > [!NOTE] In `dev`, `$env/dynamic` includes environment variables from `.env`. In `prod`, this behavior will depend on your adapter.
 * 
 * > [!NOTE] To get correct types, environment variables referenced in your code should be declared (for example in an `.env` file), even if they don't have a value until the app is deployed:
 * >
 * > ```env
 * > MY_FEATURE_FLAG=
 * > ```
 * >
 * > You can override `.env` values from the command line like so:
 * >
 * > ```sh
 * > MY_FEATURE_FLAG="enabled" npm run dev
 * > ```
 * 
 * For example, given the following runtime environment:
 * 
 * ```env
 * ENVIRONMENT=production
 * PUBLIC_BASE_URL=http://site.com
 * ```
 * 
 * With the default `publicPrefix` and `privatePrefix`:
 * 
 * ```ts
 * import { env } from '$env/dynamic/private';
 * 
 * console.log(env.ENVIRONMENT); // => "production"
 * console.log(env.PUBLIC_BASE_URL); // => undefined
 * ```
 */
declare module '$env/dynamic/private' {
	export const env: {
		NODE_ENV: string;
		npm_node_execpath: string;
		COLORTERM: string;
		CLAUDECODE: string;
		OSLogRateLimit: string;
		INFOPATH: string;
		AGENT: string;
		npm_config_user_agent: string;
		BUN_INSTALL: string;
		DEBIAN_FRONTEND: string;
		COMPOSER_NO_INTERACTION: string;
		PIP_DISABLE_PIP_VERSION_CHECK: string;
		HERDR_PANE_ID: string;
		KUBECONFIG: string;
		ATUIN_SESSION: string;
		XDG_CACHE_HOME: string;
		PIP_NO_INPUT: string;
		GH_PAGER: string;
		MANPAGER: string;
		GPG_TTY: string;
		ATUIN_HISTORY_ID: string;
		STARSHIP_SHELL: string;
		npm_package_version: string;
		SSH_ASKPASS: string;
		COREPACK_ENABLE_DOWNLOAD_PROMPT: string;
		CI: string;
		HERDR_SOCKET_PATH: string;
		CURSOR_CLI_BLOCK_CURSOR_AGENT: string;
		TERM_PROGRAM_VERSION: string;
		ATUIN_TMUX_POPUP: string;
		NODE_PATH: string;
		npm_package_name: string;
		LANG: string;
		PAGER: string;
		ITERM_PROFILE: string;
		EDITOR: string;
		LESS: string;
		BAT_PAGER: string;
		JAVA_HOME: string;
		YARN_ENABLE_TELEMETRY: string;
		DELTA_PAGER: string;
		XPC_FLAGS: string;
		npm_command: string;
		GH_PROMPT_DISABLED: string;
		LOGNAME: string;
		GLAB_PAGER: string;
		STARSHIP_SESSION_KEY: string;
		npm_lifecycle_script: string;
		npm_package_json: string;
		TERMINFO_DIRS: string;
		TERM: string;
		npm_config_progress: string;
		SYSTEMD_PAGER: string;
		NO_COLOR: string;
		SSH_AUTH_SOCK: string;
		HERDR_TAB_ID: string;
		XPC_SERVICE_NAME: string;
		TERM_SESSION_ID: string;
		CLOUDSDK_CORE_DISABLE_PROMPTS: string;
		PATH: string;
		HOME: string;
		TERM_FEATURES: string;
		npm_config_update_notifier: string;
		ATUIN_SHLVL: string;
		npm_execpath: string;
		MANPATH: string;
		TF_INPUT: string;
		VISUAL: string;
		PNPM_UPDATE_NOTIFIER: string;
		__CF_USER_TEXT_ENCODING: string;
		OMPCODE: string;
		YARN_ENABLE_PROGRESS_BARS: string;
		HERDR_ENV: string;
		PSQL_PAGER: string;
		XDG_CONFIG_HOME: string;
		GIT_TERMINAL_PROMPT: string;
		SVELTEKIT_FORK: string;
		HOMEBREW_PAGER: string;
		SHLVL: string;
		HERDR_WORKSPACE_ID: string;
		PNPM_SCRIPT_SRC_DIR: string;
		COMMAND_MODE: string;
		PYENV_SHELL: string;
		PNPM_DISABLE_SELF_UPDATE_CHECK: string;
		__CFBundleIdentifier: string;
		HOMEBREW_CELLAR: string;
		GIT_EDITOR: string;
		USER: string;
		npm_config_yes: string;
		PNPM_HOME: string;
		LC_ALL: string;
		NVM_DIR: string;
		COREPACK_ROOT: string;
		CARGO_TERM_PROGRESS_WHEN: string;
		AWS_PAGER: string;
		TMPDIR: string;
		PYTHONUNBUFFERED: string;
		HOMEBREW_PREFIX: string;
		HERDR_BIN_PATH: string;
		GIT_PAGER: string;
		HOMEBREW_REPOSITORY: string;
		TOKEN: string;
		TF_IN_AUTOMATION: string;
		npm_config_node_gyp: string;
		PWD: string;
		NVM_CD_FLAGS: string;
		NODE: string;
		COLORFGBG: string;
		INIT_CWD: string;
		MYSQL_PAGER: string;
		PYENV_ROOT: string;
		FPATH: string;
		pnpm_config_verify_deps_before_run: string;
		XDG_DATA_HOME: string;
		npm_config_fund: string;
		TERM_PROGRAM: string;
		npm_config_audit: string;
		MARKPATH: string;
		npm_lifecycle_event: string;
		[key: `PUBLIC_${string}`]: undefined;
		[key: `${string}`]: string | undefined;
	}
}

/**
 * This module provides access to environment variables set _dynamically_ at runtime and that are _publicly_ accessible.
 * 
 * |         | Runtime                                                                    | Build time                                                               |
 * | ------- | -------------------------------------------------------------------------- | ------------------------------------------------------------------------ |
 * | Private | [`$env/dynamic/private`](https://svelte.dev/docs/kit/$env-dynamic-private) | [`$env/static/private`](https://svelte.dev/docs/kit/$env-static-private) |
 * | Public  | [`$env/dynamic/public`](https://svelte.dev/docs/kit/$env-dynamic-public)   | [`$env/static/public`](https://svelte.dev/docs/kit/$env-static-public)   |
 * 
 * Dynamic environment variables are defined by the platform you're running on. For example if you're using [`adapter-node`](https://github.com/sveltejs/kit/tree/main/packages/adapter-node) (or running [`vite preview`](https://svelte.dev/docs/kit/cli)), this is equivalent to `process.env`.
 * 
 * **_Public_ access:**
 * 
 * - This module _can_ be imported into client-side code
 * - **Only** variables that begin with [`config.kit.env.publicPrefix`](https://svelte.dev/docs/kit/configuration#env) (which defaults to `PUBLIC_`) are included
 * 
 * > [!NOTE] In `dev`, `$env/dynamic` includes environment variables from `.env`. In `prod`, this behavior will depend on your adapter.
 * 
 * > [!NOTE] To get correct types, environment variables referenced in your code should be declared (for example in an `.env` file), even if they don't have a value until the app is deployed:
 * >
 * > ```env
 * > MY_FEATURE_FLAG=
 * > ```
 * >
 * > You can override `.env` values from the command line like so:
 * >
 * > ```sh
 * > MY_FEATURE_FLAG="enabled" npm run dev
 * > ```
 * 
 * For example, given the following runtime environment:
 * 
 * ```env
 * ENVIRONMENT=production
 * PUBLIC_BASE_URL=http://example.com
 * ```
 * 
 * With the default `publicPrefix` and `privatePrefix`:
 * 
 * ```ts
 * import { env } from '$env/dynamic/public';
 * console.log(env.ENVIRONMENT); // => undefined, not public
 * console.log(env.PUBLIC_BASE_URL); // => "http://example.com"
 * ```
 * 
 * ```
 * 
 * ```
 */
declare module '$env/dynamic/public' {
	export const env: {
		[key: `PUBLIC_${string}`]: string | undefined;
	}
}
