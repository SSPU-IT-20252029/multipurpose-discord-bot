# Discord Bot Architecture Overview

This document provides an overview of the two Discord bot projects in this repository: **Red-DiscordBot** (Python) and **YAGPDB** (Go). It describes their module structures, plugin/cog systems, and what to reference when implementing similar functionality.

## Table of Contents
1. [Red-DiscordBot (Python)](#red-discordbot-python)
   - [Core Architecture](#core-architecture)
   - [Config System](#config-system)
   - [Cogs / Modules](#cogs--modules)
   - [Command System](#command-system)
   - [i18n / Localization](#i18n--localization)
   - [Drivers / Storage](#drivers--storage)
   - [Key Reference Files](#key-reference-files)
2. [YAGPDB (Go)](#yagpdb-go)
   - [Core Architecture](#core-architecture-1)
   - [Plugin System](#plugin-system)
   - [Command System](#command-system-1)
   - [Web / Control Panel](#web--control-panel)
   - [Background Workers](#background-workers)
   - [Storage](#storage)
   - [Key Reference Files](#key-reference-files-1)
3. [Cross-Project Inspiration Guide](#cross-project-inspiration-guide)

## 1. Red-DiscordBot (Python)

Red is a large, modular, multi-purpose Discord bot written in Python using discord.py.
It is split into a **core** (always loaded) and **cogs** (loadable extensions).

### Core Architecture

- `redbot/__init__.py` - Version info, early init hooks (logging, colorama).
- `redbot/core/bot.py` - The `Red` bot class. Inherits from
  `discord.ext.commands.AutoShardedBot` plus mixins (`RPCMixin`, `RedTree`).
  This is the main runtime entrypoint.
- `redbot/__main__.py` - CLI entrypoint that parses args and launches the bot.
- `redbot/core/_cli.py` - CLI flag parsing and exit codes.
- `redbot/core/_cog_manager.py` - Cog loading/unloading UI logic.
- `redbot/core/_events.py` - Registers core Discord event handlers.
- `redbot/core/_global_checks.py` - Global command checks (permissions, ratelimits).
- `redbot/core/_rpc.py` - RPC server for inter-process communication.
- `redbot/core/_settings_caches.py` - In-memory caches for prefix, ignored cogs,
  i18n locale, whitelist/blacklist.
- `redbot/core/_diagnoser.py` - Debug/diagnostic tooling.
- `redbot/core/_debuginfo.py` - Debug info collection.
- `redbot/core/_sharedlibdeprecation.py` - Deprecation handling for shared libs.
- `redbot/core/tree.py` - Custom command tree (`RedTree`).
- `redbot/core/data_manager.py` - Paths for storing cog data.
- `redbot/core/dev_commands.py` - The `Dev` cog for developers.
- `redbot/core/core_commands.py` - The `Core` cog with built-in commands.
- `redbot/core/errors.py` - Custom exception hierarchy.
- `redbot/core/bank.py` - Bank/currency abstraction layer.
- `redbot/core/modlog.py` - Moderation logging system.
- `redbot/core/i18n.py`, `redbot/core/_i18n.py` - Internationalization machinery.
- `redbot/core/generic_casetypes.py` - Generic case type definitions.

### Command System

- `redbot/core/commands/` - Custom command framework built on discord.py.
  - `commands.py` - Command/group definitions, decorators.
  - `context.py` - Custom `Context` object.
  - `converter.py` - Custom argument converters.
  - `requires.py` - Permission requirement decorators.
  - `help.py` - Help formatter.
  - `errors.py` - Command error classes.
- `redbot/core/app_commands/` - Application (slash) command support.
  - `checks.py`, `errors.py`.

### Config System

The `Config` is the central per-guild/per-channel/per-user settings store.

- `redbot/core/config.py` - `Config` class with grouped hierarchical data
  (global -> guild -> channel -> member -> user). Uses a singleton cache
  (`ConfigMeta` metaclass). Supports defaults, registration, and async
  context managers for mutable values.
- `redbot/core/_drivers/` - Pluggable storage backends.
  - `base.py` - `BaseDriver`, `ConfigCategory`, `IdentifierData`.
  - `json.py` - JSON file driver (default).
  - `postgres/postgres.py` - Postgres driver.
  - `_mongo.py` - MongoDB driver (optional).
- Config patterns to copy:
  - `Config.get_conf(self, identifier, force_registration=True)`
  - `self.config.register_global(...)`, `register_guild(...)`, etc.
  - `await self.config.guild(guild).foo()`, `await self.config.guild(guild).foo.set(x)`
  - `async with self.config.guild(guild).bar() as data:`

### Cogs / Modules

Cogs live under `redbot/cogs/`. Each cog is a Python package with a main module
named after the cog. They subclass `redbot.core.commands.Cog`.

Standard cog structure (see `redbot/cogs/alias/alias.py`):
- Class decorated with `@cog_i18n(_)` for translation support.
- `__init__(self, bot: Red)` - stores `self.bot`, creates `self.config`.
- `cog_load()` async hook called after loading.
- `cog_unload()` for cleanup.
- `red_delete_data_for_user(...)` for GDPR-compliant data deletion.
- Commands defined with `@commands.command()` and `@commands.group()`.

Cogs included:
- `admin/` - Server admin settings, announcer, converters.
- `alias/` - Command aliases (`alias.py`, `alias_entry.py`).
- `audio/` - Lavalink music player (large: `manager.py`, `core/`, `apis/`,
  `managed_node/`).
- `cleanup/` - Bulk message cleanup.
- `customcom/` - Custom commands.
- `downloader/` - Installable cog repository manager (`repo_manager.py`,
  `installable.py`).
- `economy/` - Currency/bank.
- `filter/` - Word filter.
- `general/` - General utility commands.
- `image/` - Image commands.
- `mod/` - Moderation (kick, ban, slowmode, names). Uses mixin pattern
  (`kickban.py`, `slowmode.py`, `settings.py`, `events.py`, `names.py`).
- `modlog/` - Modlog entries.
- `mutes/` - Role and voice mutes (`mutes.py`, `voicemutes.py`).
- `permissions/` - Command permission overrides.
- `reports/` - Reporting system.
- `streams/` - Stream notifications.
- `trivia/` - Trivia game with YAML question lists.
- `warnings/` - Warning system.

### i18n / Localization

- `redbot/core/i18n.py` - `Translator` and `cog_i18n` decorator.
- Translation files use gettext `.po`/`.mo` format, stored per cog in
  `locales/<LC>.po` (e.g. `redbot/cogs/admin/locales/en-US.po`).
- Use `_ = Translator("CogName", __file__)` then `_("string")` for translatable
  text.

### Drivers / Storage

- `redbot/core/_drivers/` - Storage backends: JSON (default), Postgres, MongoDB.
- `redbot/core/_drivers/json.py` is the reference implementation for a driver.

### Key Reference Files

- `redbot/core/bot.py` - Main bot class.
- `redbot/core/config.py` - Config system.
- `redbot/core/commands/commands.py` - Command framework.
- `redbot/cogs/alias/alias.py` - Minimal example cog.
- `redbot/cogs/mod/mod.py` - Composite cog with mixins.
- `redbot/cogs/audio/manager.py` - Large feature cog example.
- `redbot/cogs/downloader/repo_manager.py` - External plugin management.## 2. YAGPDB (Go)

YAGPDB is a modular Discord bot written in Go using a custom fork of discordgo.
It uses a **plugin system** where each plugin registers itself and implements
optional lifecycle interfaces. It also includes a web control panel.

### Core Architecture

- `cmd/yagpdb/main.go` - Entry point. Calls `run.Init()` then registers all
  plugins via `X.RegisterPlugin()`, then `run.Run()`.
- `cmd/shardorchestrator/main.go` - Sharding orchestrator process.
- `cmd/capturepanics/main.go` - Panic capture helper.
- `bot/bot.go` - The bot core: sharding, gateway intents, event handlers,
  standalone vs orchestrator mode.
- `bot/plugin.go` - Plugin lifecycle interfaces and `BotPlugin` core instance.
  Interfaces: `BotInitHandler`, `LateBotInitHandler`, `NewGuildHandler`,
  `RemoveGuildHandler`, `BotStopperHandler`, `ShardMigrationHandler`.
- `bot/eventsystem/` - Event dispatcher (`events.go`, `eventsystem.go`).
- `bot/discordevents.go` - discordgo event wiring.
- `bot/botrest/` - Bot REST API server/client for cross-process calls.
- `bot/paginatedmessages/` - Paginated message helper.
- `bot/shardmemberfetcher/` - Batch member fetching for sharding.

### Plugin System

Plugins live in top-level packages (e.g. `autorole/`, `commands/`, `automod/`).

Each plugin follows this pattern (see `autorole/autorole.go`):

```go
type Plugin struct{}

func (p *Plugin) PluginInfo() *common.PluginInfo {
    return &common.PluginInfo{
        Name:     "Autorole",
        SysName:  "autorole",
        Category: common.PluginCategoryMisc,
    }
}

func RegisterPlugin() {
    p := &Plugin{}
    common.RegisterPlugin(p)
}
```

- `common/plugins.go` - `Plugin` interface, `RegisterPlugin`, `PluginInfo`,
  `PluginCategory*`, `PluginWithCommonRun`.
- Plugins optionally implement lifecycle interfaces from `bot/plugin.go`:
  - `BotInit()` - startup
  - `LateBotInit()` - after core init
  - `NewGuild(guild)` - new server setup
  - `RemoveGuild(guildID)` - cleanup on leave
  - `StopBot(wg)` - graceful shutdown
- Many plugins split into `plugin_bot.go` (bot lifecycle) and `plugin_web.go`
  (web control panel routes).

### Command System

- `commands/` - Core command system.
  - `commands.go` - `CommandSystem *dcmd.System`, command registration.
  - `yagcommmand.go` - YAG command wrapper with permission checks.
  - `slashcommands.go` - Slash command syncing.
  - `help.go` - Help formatter.
  - `tmplexec.go` - Execution middleware.
  - `schema.go`, `models/` - SQL schema and generated models for command overrides.
- `dcmd` lives under `lib/dcmd` (external dependency, but the reference for
  command definitions is in `commands/plugin_bot.go`).
- Custom commands plugin: `customcommands/` - user-defined text/slash/context
  commands with templates (`handle_text.go`, `handle_slashcommand.go`, etc.).

### Web / Control Panel

- `frontend/` - Go HTML templates + static assets (Bootstrap, DataTables,
  Font Awesome, CodeMirror). `frontend/frontend.go` wires routes.
- `web/` (separate module) - HTTP server, auth, sidebar items, templates.
- Each plugin with a control panel has a `plugin_web.go` with `InitWeb()`
  that registers HTML templates, sidebar items, and HTTP routes (see
  `commands/plugin_web.go`).
- `admin/web.go` - Admin settings panel.
- `common/templates/` - Template helpers and structs.

### Background Workers

- `common/backgroundworkers/backgroundworkers.go` - Background worker registry.
- `common/mqueue/` - Message queue backed by Redis (producer, worker, bot
  processor). Used for async task processing.
- `common/run/run.go` - Main runner that starts bot, web server, background
  workers.
- `common/scheduledevents2/` - Scheduled events (cron-like) with DB persistence.
- `common/featureflags/` - Per-guild feature flags with caching.
- `common/cacheset/` - Generic cache slot registration (`CacheSet.RegisterSlot`).

### Storage

- `common/models/` - Generated SQL models (sqlboiler) for core tables.
- Each plugin with DB needs its own `models/` subpackage and a `schema.go`
  with `InitSchemas` (e.g. `commands/schema.go`, `customcommands/schema.go`).
- `common/config/` - Configuration system (`config.go`, `envsource.go`,
  `redissource.go`, `singleton.go`). Options registered via
  `config.RegisterOption(key, description, defaultValue)`.
- Redis used for caching, locks (`common/redislock.go`), rate limits
  (`common/multiratelimit/`), message queue (`common/mqueue/`).
- Postgres is the primary SQL DB (`common/pqkeydb`, `bot/models`,
  `commands/models`, etc.).

### Key Reference Files

- `cmd/yagpdb/main.go` - Plugin registration order.
- `bot/plugin.go` - Lifecycle interfaces.
- `bot/bot.go` - Bot core, sharding, intents.
- `commands/plugin_bot.go` - Command system init and middleware.
- `commands/plugin_web.go` - Web panel example with route registration.
- `autorole/autorole.go` - Minimal plugin example.
- `customcommands/` - Full custom command plugin (text, slash, context, timed,
  interval).
- `automod/` - Complex plugin with rules, triggers, conditions, effects.
- `common/plugins.go` - Plugin registry.
- `common/backgroundworkers/backgroundworkers.go` - BGW pattern.
- `common/run/run.go` - Service runner.## 3. Cross-Project Inspiration Guide

When writing a new module for a Discord bot, here is what to look at in each project.

### Want a modular plugin/cog architecture?

- **Python (Red)**: Study `redbot/core/bot.py` for how the bot loads and manages
  cogs, and `redbot/cogs/alias/alias.py` for the canonical minimal cog. Use
  `redbot.core.config.Config` for per-guild settings and
  `redbot.core.commands` for command definitions.
- **Go (YAGPDB)**: Study `common/plugins.go` for the plugin registry and
  `autorole/autorole.go` for a minimal plugin. Implement lifecycle interfaces
  from `bot/plugin.go` (`BotInitHandler`, `NewGuildHandler`,
  `RemoveGuildHandler`, `BotStopperHandler`) as needed.

### Want a settings/config system with per-guild defaults?

- **Red**: `redbot/core/config.py` + `redbot/core/_drivers/`. The pattern is
  `Config.get_conf(self, identifier)` then `register_global/guild/channel/...`.
  Defaults are declared inline. Drivers (JSON, Postgres) implement
  `BaseDriver`.
- **YAGPDB**: `common/config/config.go` with `config.RegisterOption(key, desc,
  default)`. Values cached in Redis via `common/cacheset`. SQL models in
  `common/models/` for persistent config.

### Want a command framework with permissions?

- **Red**: `redbot/core/commands/commands.py` and `requires.py`. Commands are
  methods on `Cog` subclasses. Permissions use `@commands.check` /
  `@commands.bot_has_permissions` etc. See `redbot/cogs/mod/mod.py`.
- **YAGPDB**: `commands/plugin_bot.go` and `lib/dcmd` (external). The
  `YAGCommand` wrapper adds permission checks (`checkCanExecuteCommand`).
  Override middleware via `YAGCommandMiddleware`. Slash commands in
  `commands/slashcommands.go`.

### Want a web control panel for settings?

- **YAGPDB** is the reference. Study:
  - `commands/plugin_web.go` - template embedding, sidebar items, route
    registration with `goji.SubMux()`.
  - `frontend/frontend.go` - template and static file wiring.
  - `web/` package - auth, `ControllerHandler`, `ControllerPostHandler`.
  - `common/templates/` - template helper functions.

### Want background workers / scheduled tasks?

- **YAGPDB**: `common/backgroundworkers/backgroundworkers.go` for worker registry,
  `common/scheduledevents2/` for scheduled events with DB persistence,
  `common/mqueue/` for Redis-backed message queue. See `common/run/run.go` for
  how everything is started.

### Want i18n / translations?

- **Red**: `redbot/core/i18n.py`. Use `_(Translator("Cog", __file__))` and
  `@cog_i18n(_)` on the cog class. Translation files are `.po` files in
  `cogs/<name>/locales/<LC>.po`.

### Want moderation logging?

- **Red**: `redbot/core/modlog.py` and `redbot/cogs/modlog/modlog.py`. Cases are
  registered via `core.modlog.register_casetype(...)`.
- **YAGPDB**: `bot/eventlogger.go` and the `logs/` plugin.

### Want external plugin/downloader support?

- **Red**: `redbot/cogs/downloader/` - `repo_manager.py`, `installable.py`,
  `json_mixins.py`, `info_schemas.py`. This is the canonical implementation of
  installing/uninstalling third-party cogs from git repos.

### Want audio / music playback?

- **Red**: `redbot/cogs/audio/` - a very large, complete implementation using
  Lavalink. Key files: `manager.py` (player management), `core/commands/`
  (command surface), `apis/` (YouTube/Spotify/playlist sources), `managed_node/`
  (Lavalink server management).

### Want antiphishing / automod?

- **YAGPDB**: `antiphishing/antiphishing.go` and `automod/` (rules, triggers,
  conditions, effects). The `automod/models/` subpackage holds generated SQL
  models for rules.

### Want a verification / role assignment flow?

- Look at the main project README for the current verification bot pattern
  (button -> modal -> email -> code -> role). For role assignment logic:
  - **Red**: `redbot/cogs/autorole` equivalent would use `Config` per guild and
    a `MemberJoin` event handler.
  - **YAGPDB**: `autorole/autorole.go` - config cached in Redis via
    `common.CacheSet.RegisterSlot`, applied on guild join and optionally on
    screening completion.