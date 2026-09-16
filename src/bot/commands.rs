//! Slash command handlers.
//!
//! Parity target: Go `cmd/bot/main.go` command handlers. Sessions 7–8 add
//! `/ratelimit`, `/verifiedrole`, `/language`, and `/backup`.

use crate::bot::interactions::BTN_VERIFY_START;
use crate::bot::{Bot, respond_err, respond_ok};
use crate::error::Error;
use crate::i18n;
use crate::store::GuildConfig;
use poise::{ApplicationContext, CreateReply};
use serenity::all::{
    self as serenity, ButtonStyle, CreateActionRow, CreateButton, CreateEmbed, CreateMessage,
};

const NANOS_PER_SEC: i64 = 1_000_000_000;
/// Defaults (Go `/setup`): code TTL 10 min, rate limit 3 / 15 min.
const DEFAULT_TTL_NS: i64 = 10 * 60 * NANOS_PER_SEC;
const DEFAULT_WINDOW_NS: i64 = 15 * 60 * NANOS_PER_SEC;

/// `/help` — ephemeral embed listing all commands.
///
/// Parity: Go `cmdHelp` — exact description layout and color `0x3b82f6`.
/// Note: `/backup` is intentionally absent (Go parity).
#[poise::command(slash_command)]
pub async fn help(ctx: ApplicationContext<'_, Bot, Error>) -> Result<(), Error> {
    let t = i18n::get(
        ctx.data()
            .locale(ctx.guild_id().map(|g| g.get()), ctx.author().id.get()),
    );

    let description = format!(
        "{hint}\n\n**{admin}**\n`/setup` - {setup}\n`/regex` - {regex}\n`/csv` - {csv}\n`/ratelimit` - {ratelimit}\n`/verifiedrole` - {verifiedrole}\n\n**{user}**\n`/language` - {language}\n`/help` - {help}",
        hint = t.help_click_hint,
        admin = t.help_admin_title,
        setup = t.setup_desc,
        regex = t.regex_desc,
        csv = t.csv_desc,
        ratelimit = t.rate_limit_desc,
        verifiedrole = t.verified_role_desc,
        user = t.help_user_title,
        language = t.language_desc,
        help = t.help_desc,
    );

    ctx.send(
        CreateReply::default()
            .embed(
                CreateEmbed::default()
                    .title(t.help_text)
                    .description(description)
                    .color(0x3b82f6),
            )
            .ephemeral(true),
    )
    .await?;
    Ok(())
}

/// `/setup` — configure verification for the server.
///
/// Saves the guild config and posts the verify embed + button. Preserves
/// `default_role_id` across re-setup (Go `cmdSetup` parity).
#[poise::command(slash_command, default_member_permissions = "ADMINISTRATOR")]
pub async fn setup(
    ctx: ApplicationContext<'_, Bot, Error>,
    domain: String,
    #[choices("REGEX", "CSV")] mode: &'static str,
    channel: serenity::Channel,
    subject: Option<String>,
) -> Result<(), Error> {
    let t = i18n::get(
        ctx.data()
            .locale(ctx.guild_id().map(|g| g.get()), ctx.author().id.get()),
    );
    let guild_id = ctx
        .guild_id()
        .ok_or_else(|| Error::Message("setup must be run inside a server".into()))?
        .to_string();

    let subject = subject.unwrap_or_else(|| t.default_subject.to_string());
    let existing_default = ctx
        .data()
        .store
        .get_guild_config(&guild_id)?
        .map(|c| c.default_role_id)
        .unwrap_or_default();

    let cfg = GuildConfig {
        guild_id,
        verify_channel_id: channel.id().to_string(),
        domain: domain.clone(),
        mode: mode.to_string(),
        subject,
        code_ttl_ns: DEFAULT_TTL_NS,
        max_attempts: 5,
        rate_limit_count: 3,
        rate_limit_window_ns: DEFAULT_WINDOW_NS,
        default_role_id: existing_default,
    };
    if let Err(e) = ctx.data().store.save_guild_config(&cfg) {
        tracing::error!(%e, "failed to save guild config");
        return respond_err(ctx, t.failed_save.to_string()).await;
    }

    let embed = CreateEmbed::default()
        .title(t.embed_title)
        .description(i18n::subst(t.embed_desc_fmt, &[domain.as_str()]))
        .color(0x3b82f6);
    let row = CreateActionRow::Buttons(vec![
        CreateButton::new(BTN_VERIFY_START)
            .label(t.verify_btn)
            .style(ButtonStyle::Primary),
    ]);
    let result = channel
        .id()
        .send_message(
            &ctx.serenity_context.http,
            CreateMessage::new().embed(embed).components(vec![row]),
        )
        .await;

    match result {
        Ok(_) => respond_ok(ctx, t.config_saved.to_string()).await,
        Err(e) => {
            tracing::error!(%e, "failed to post verify message");
            respond_err(ctx, t.config_saved_err.to_string()).await
        }
    }
}

/// `/regex` — manage per-guild regex rules.
#[poise::command(
    slash_command,
    subcommands("add", "list", "remove"),
    default_member_permissions = "ADMINISTRATOR"
)]
pub async fn regex(ctx: ApplicationContext<'_, Bot, Error>) -> Result<(), Error> {
    let t = i18n::get(
        ctx.data()
            .locale(ctx.guild_id().map(|g| g.get()), ctx.author().id.get()),
    );
    respond_ok(ctx, t.regex_desc.to_string()).await
}

/// Add a regex rule. Parity: Go `cmdRegex` `add`.
#[poise::command(slash_command)]
async fn add(
    ctx: ApplicationContext<'_, Bot, Error>,
    pattern: String,
    role: serenity::Role,
    priority: Option<i64>,
) -> Result<(), Error> {
    let t = i18n::get(
        ctx.data()
            .locale(ctx.guild_id().map(|g| g.get()), ctx.author().id.get()),
    );
    let guild_id = ctx
        .guild_id()
        .ok_or_else(|| Error::Message("must be run inside a server".into()))?
        .to_string();
    if let Err(e) = ctx.data().store.add_regex_rule(
        &guild_id,
        &pattern,
        &role.id.to_string(),
        priority.unwrap_or(0),
    ) {
        tracing::error!(%e, "failed to add regex rule");
        return respond_err(ctx, t.failed_save.to_string()).await;
    }
    respond_ok(ctx, t.rule_added.to_string()).await
}

/// List all regex rules (priority desc). Parity: Go `cmdRegex` `list`.
#[poise::command(slash_command)]
async fn list(ctx: ApplicationContext<'_, Bot, Error>) -> Result<(), Error> {
    let t = i18n::get(
        ctx.data()
            .locale(ctx.guild_id().map(|g| g.get()), ctx.author().id.get()),
    );
    let guild_id = ctx
        .guild_id()
        .ok_or_else(|| Error::Message("must be run inside a server".into()))?
        .to_string();
    let rules = match ctx.data().store.list_regex_rules(&guild_id) {
        Ok(r) => r,
        Err(e) => {
            tracing::error!(%e, "failed to load regex rules");
            return respond_err(ctx, t.failed_load_rules.to_string()).await;
        }
    };
    if rules.is_empty() {
        return respond_ok(ctx, t.no_rules.to_string()).await;
    }
    let mut msg = String::new();
    for r in rules {
        msg.push_str(&format!(
            "ID: {} | Pattern: `{}` | Role: <@&{}> | Priority: {}\n",
            r.id, r.pattern, r.role_id, r.priority
        ));
    }
    respond_ok(ctx, msg).await
}

/// Remove a regex rule by id. Parity: Go `cmdRegex` `remove`.
#[poise::command(slash_command)]
async fn remove(ctx: ApplicationContext<'_, Bot, Error>, id: i64) -> Result<(), Error> {
    let t = i18n::get(
        ctx.data()
            .locale(ctx.guild_id().map(|g| g.get()), ctx.author().id.get()),
    );
    if let Err(e) = ctx.data().store.remove_regex_rule(id) {
        tracing::error!(%e, "failed to delete regex rule");
        return respond_err(ctx, t.failed_delete.to_string()).await;
    }
    respond_ok(ctx, t.rule_deleted.to_string()).await
}

/// `/csv` — manage uploaded CSV email→class data and class→role mappings.
#[poise::command(
    slash_command,
    subcommands("upload", "map"),
    default_member_permissions = "ADMINISTRATOR"
)]
pub async fn csv(ctx: ApplicationContext<'_, Bot, Error>) -> Result<(), Error> {
    let t = i18n::get(
        ctx.data()
            .locale(ctx.guild_id().map(|g| g.get()), ctx.author().id.get()),
    );
    respond_ok(ctx, t.csv_desc.to_string()).await
}

/// Upload a CSV (`email,class`) and replace the guild's existing rows.
///
/// Parity: Go `cmdCSV` `upload` — destructive clear, no header skip, empty
/// email/class skipped.
#[poise::command(slash_command)]
async fn upload(
    ctx: ApplicationContext<'_, Bot, Error>,
    file: serenity::Attachment,
) -> Result<(), Error> {
    let t = i18n::get(
        ctx.data()
            .locale(ctx.guild_id().map(|g| g.get()), ctx.author().id.get()),
    );
    let guild_id = ctx
        .guild_id()
        .ok_or_else(|| Error::Message("must be run inside a server".into()))?
        .to_string();

    let body = match reqwest::get(&file.url).await {
        Ok(resp) if resp.status().is_success() => match resp.bytes().await {
            Ok(b) => b.to_vec(),
            Err(_) => return respond_err(ctx, t.error_download.to_string()).await,
        },
        _ => return respond_err(ctx, t.error_download.to_string()).await,
    };

    let mut reader = csv::ReaderBuilder::new()
        .has_headers(false)
        .from_reader(body.as_slice());
    let mut records: Vec<Vec<String>> = Vec::new();
    for record in reader.records() {
        match record {
            Ok(r) => records.push(r.iter().map(String::from).collect()),
            Err(_) => return respond_err(ctx, t.invalid_csv.to_string()).await,
        }
    }

    let store = &ctx.data().store;
    if let Err(e) = store.clear_csv_emails(&guild_id) {
        tracing::error!(%e, "failed to clear csv emails");
        return respond_err(ctx, t.failed_save.to_string()).await;
    }
    let mut count = 0i64;
    for row in &records {
        if row.len() >= 2 {
            let email = row[0].trim();
            let class = row[1].trim();
            if !email.is_empty() && !class.is_empty() {
                if let Err(e) = store.insert_csv_email(&guild_id, email, class) {
                    tracing::error!(%e, "failed to insert csv email");
                    return respond_err(ctx, t.failed_save.to_string()).await;
                }
                count += 1;
            }
        }
    }

    respond_ok(
        ctx,
        i18n::subst(t.uploaded_emails_fmt, &[&count.to_string()]),
    )
    .await
}

/// Map a CSV class name to a Discord role (upsert).
///
/// Parity: Go `cmdCSV` `map`.
#[poise::command(slash_command)]
async fn map(
    ctx: ApplicationContext<'_, Bot, Error>,
    class: String,
    role: serenity::Role,
) -> Result<(), Error> {
    let t = i18n::get(
        ctx.data()
            .locale(ctx.guild_id().map(|g| g.get()), ctx.author().id.get()),
    );
    let guild_id = ctx
        .guild_id()
        .ok_or_else(|| Error::Message("must be run inside a server".into()))?
        .to_string();
    if let Err(e) = ctx
        .data()
        .store
        .map_csv_class(&guild_id, &class, &role.id.to_string())
    {
        tracing::error!(%e, "failed to map csv class");
        return respond_err(ctx, t.failed_map.to_string()).await;
    }
    respond_ok(
        ctx,
        i18n::subst(t.class_mapped_fmt, &[class.as_str(), &role.id.to_string()]),
    )
    .await
}

const NANOS_PER_MIN: i64 = 60 * 1_000_000_000;

/// `/ratelimit` — set the email send rate limit (count per window minutes).
///
/// Parity: Go `cmdRateLimit` — validation, EN `ErrMissingConfig` on unset guild,
/// reply `RateLimitSetFmt count / window min.`.
#[poise::command(slash_command, default_member_permissions = "ADMINISTRATOR")]
pub async fn ratelimit(
    ctx: ApplicationContext<'_, Bot, Error>,
    count: i64,
    window: i64,
) -> Result<(), Error> {
    let t = i18n::get(
        ctx.data()
            .locale(ctx.guild_id().map(|g| g.get()), ctx.author().id.get()),
    );
    let guild_id = ctx
        .guild_id()
        .ok_or_else(|| Error::Message("must be run inside a server".into()))?
        .to_string();

    if !(1..=3).contains(&count) || !(15..=60).contains(&window) {
        return respond_err(ctx, t.failed_save.to_string()).await;
    }

    let mut cfg = match ctx.data().store.get_guild_config(&guild_id)? {
        Some(c) => c,
        None => {
            // Parity: Go replies with the hardcoded English ErrMissingConfig here.
            let en = i18n::get(i18n::Locale::En);
            return respond_err(ctx, en.err_missing_config.to_string()).await;
        }
    };
    cfg.rate_limit_count = count;
    cfg.rate_limit_window_ns = window * NANOS_PER_MIN;

    if let Err(e) = ctx.data().store.save_guild_config(&cfg) {
        tracing::error!(%e, "failed to save rate limit config");
        return respond_err(ctx, t.failed_save.to_string()).await;
    }

    respond_ok(
        ctx,
        format!("{} {count} / {window} min.", t.rate_limit_set_fmt),
    )
    .await
}

/// `/verifiedrole` — manage the default role assigned to every verified user.
#[poise::command(
    slash_command,
    subcommands("set", "view", "clear"),
    default_member_permissions = "ADMINISTRATOR"
)]
pub async fn verifiedrole(ctx: ApplicationContext<'_, Bot, Error>) -> Result<(), Error> {
    let t = i18n::get(
        ctx.data()
            .locale(ctx.guild_id().map(|g| g.get()), ctx.author().id.get()),
    );
    respond_ok(ctx, t.verified_role_desc.to_string()).await
}

async fn load_cfg(
    ctx: &ApplicationContext<'_, Bot, Error>,
    t: i18n::Translations,
) -> Result<Option<GuildConfig>, ()> {
    let guild_id = match ctx.guild_id() {
        Some(g) => g.to_string(),
        None => return Err(()),
    };
    match ctx.data().store.get_guild_config(&guild_id) {
        Ok(Some(c)) => Ok(Some(c)),
        Ok(None) => {
            // Parity: Go replies with the hardcoded English ErrMissingConfig.
            let en = i18n::get(i18n::Locale::En);
            let _ = respond_err(*ctx, en.err_missing_config.to_string()).await;
            Ok(None)
        }
        Err(e) => {
            tracing::error!(%e, "failed to load guild config");
            let _ = respond_err(*ctx, t.failed_save.to_string()).await;
            Ok(None)
        }
    }
}

/// Set the default verified role.
#[poise::command(slash_command)]
async fn set(ctx: ApplicationContext<'_, Bot, Error>, role: serenity::Role) -> Result<(), Error> {
    let t = i18n::get(
        ctx.data()
            .locale(ctx.guild_id().map(|g| g.get()), ctx.author().id.get()),
    );
    let Ok(Some(mut cfg)) = load_cfg(&ctx, t).await else {
        return Ok(());
    };
    cfg.default_role_id = role.id.to_string();
    if let Err(e) = ctx.data().store.save_guild_config(&cfg) {
        tracing::error!(%e, "failed to save verified role");
        return respond_err(ctx, t.failed_save.to_string()).await;
    }
    respond_ok(
        ctx,
        i18n::subst(t.verified_role_set_fmt, &[&role.id.to_string()]),
    )
    .await
}

/// Show the current default verified role.
#[poise::command(slash_command)]
async fn view(ctx: ApplicationContext<'_, Bot, Error>) -> Result<(), Error> {
    let t = i18n::get(
        ctx.data()
            .locale(ctx.guild_id().map(|g| g.get()), ctx.author().id.get()),
    );
    let Ok(Some(cfg)) = load_cfg(&ctx, t).await else {
        return Ok(());
    };
    if cfg.default_role_id.is_empty() {
        return respond_ok(ctx, t.verified_role_not_set.to_string()).await;
    }
    respond_ok(
        ctx,
        i18n::subst(t.verified_role_view_fmt, &[cfg.default_role_id.as_str()]),
    )
    .await
}

/// Clear the default verified role.
#[poise::command(slash_command)]
async fn clear(ctx: ApplicationContext<'_, Bot, Error>) -> Result<(), Error> {
    let t = i18n::get(
        ctx.data()
            .locale(ctx.guild_id().map(|g| g.get()), ctx.author().id.get()),
    );
    let Ok(Some(mut cfg)) = load_cfg(&ctx, t).await else {
        return Ok(());
    };
    cfg.default_role_id.clear();
    if let Err(e) = ctx.data().store.save_guild_config(&cfg) {
        tracing::error!(%e, "failed to clear verified role");
        return respond_err(ctx, t.failed_save.to_string()).await;
    }
    respond_ok(ctx, t.verified_role_cleared.to_string()).await
}

/// `/language` — set your per-guild bot language (en / cs).
///
/// Parity: Go `cmdLanguage` — stores `en`/`cs`; the confirmation uses the
/// *previous* locale's `LanguageSetFmt` (Go quirk, mirrored).
#[poise::command(slash_command)]
pub async fn language(
    ctx: ApplicationContext<'_, Bot, Error>,
    #[choices("en", "cs")] language: &'static str,
) -> Result<(), Error> {
    let t = i18n::get(
        ctx.data()
            .locale(ctx.guild_id().map(|g| g.get()), ctx.author().id.get()),
    );
    let (guild_id, user_id) = match ctx.guild_id() {
        Some(g) => (g.to_string(), ctx.author().id.to_string()),
        None => return respond_err(ctx, t.failed_save.to_string()).await,
    };

    let locale = i18n::Locale::parse(language);
    if let Err(e) = ctx
        .data()
        .store
        .set_user_locale(&guild_id, &user_id, locale.code())
    {
        tracing::error!(%e, "failed to save user locale");
        return respond_err(ctx, t.failed_save.to_string()).await;
    }

    let lang_name = match locale {
        i18n::Locale::En => "English",
        i18n::Locale::Cs => "Čeština",
    };
    respond_ok(ctx, i18n::subst(t.language_set_fmt, &[lang_name])).await
}
