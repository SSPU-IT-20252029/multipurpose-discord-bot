//! Slash command handlers.
//!
//! Parity target: Go `cmd/bot/main.go` command handlers. Sessions 6–8 add
//! `/setup`, `/regex`, `/csv`, `/ratelimit`, `/verifiedrole`, `/backup`.

use crate::bot::Bot;
use crate::error::Error;
use crate::i18n;
use poise::serenity_prelude::CreateEmbed;
use poise::{ApplicationContext, CreateReply};

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
