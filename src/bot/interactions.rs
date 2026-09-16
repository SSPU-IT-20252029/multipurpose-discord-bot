//! Interaction dispatch for message components and modal submissions.
//!
//! poise 0.6 routes slash commands natively but not buttons/modals, so these
//! are handled here through the framework's `event_handler`.
//!
//! Parity target: Go `cmd/bot/main.go` `handleComponent` + `handleModal`.

use crate::bot::Bot;
use crate::error::Error;
use crate::i18n;
use serenity::all::{
    self as serenity, ButtonStyle, CreateActionRow, CreateButton, CreateInputText,
    CreateInteractionResponse, CreateInteractionResponseMessage, CreateModal, InputTextStyle,
};

pub const BTN_VERIFY_START: &str = "btn_verify_start";
pub const BTN_ENTER_CODE: &str = "btn_enter_code";
pub const MODAL_EMAIL: &str = "modal_email";
pub const INPUT_EMAIL: &str = "input_email";
pub const MODAL_CODE: &str = "modal_code";
pub const INPUT_CODE: &str = "input_code";

fn locale_of(
    data: &Bot,
    guild_id: Option<serenity::GuildId>,
    user_id: &serenity::UserId,
) -> i18n::Locale {
    data.locale(guild_id.map(|g| g.get()), user_id.get())
}

/// Route component/modal interactions (parity: Go `onInteractionCreate` switch).
pub async fn handle_interaction(
    ctx: &serenity::Context,
    interaction: &serenity::Interaction,
    data: &Bot,
) -> Result<(), Error> {
    if data.debug {
        let (guild_id, user_id, custom_id) = match interaction {
            serenity::Interaction::Component(c) => {
                (c.guild_id, Some(c.user.id), Some(c.data.custom_id.clone()))
            }
            serenity::Interaction::Modal(m) => {
                (m.guild_id, Some(m.user.id), Some(m.data.custom_id.clone()))
            }
            _ => (None, None, None),
        };
        if let Some(custom_id) = custom_id {
            tracing::debug!(
                guild = %guild_id.map(|g| g.to_string()).unwrap_or_default(),
                user = %user_id.map(|u| u.to_string()).unwrap_or_default(),
                custom_id,
                "interaction received"
            );
        }
    }

    match interaction.kind() {
        serenity::InteractionType::Component => {
            if let Some(component) = interaction.as_message_component()
                && component.data.custom_id == BTN_VERIFY_START
            {
                show_email_modal(ctx, component, data).await?;
            }
        }
        serenity::InteractionType::Modal => {
            if let Some(modal) = interaction.as_modal_submit()
                && modal.data.custom_id == MODAL_EMAIL
            {
                submit_email(ctx, modal, data).await?;
            }
        }
        _ => {}
    }
    Ok(())
}

/// Parity: Go `btn_verify_start` → open `modal_email` with a single text input.
async fn show_email_modal(
    ctx: &serenity::Context,
    component: &serenity::ComponentInteraction,
    data: &Bot,
) -> Result<(), Error> {
    let t = i18n::get(locale_of(data, component.guild_id, &component.user.id));
    let row = CreateActionRow::InputText(
        CreateInputText::new(InputTextStyle::Short, t.your_email, INPUT_EMAIL)
            .placeholder(t.email_placeholder),
    );
    let modal = CreateModal::new(MODAL_EMAIL, t.verify_modal_title).components(vec![row]);
    component
        .create_response(ctx, CreateInteractionResponse::Modal(modal))
        .await?;
    Ok(())
}

/// Parity: Go `modal_email` handler → `verify.Start`, reply with "code sent" +
/// the Enter Code button, or the localized start error.
async fn submit_email(
    ctx: &serenity::Context,
    modal: &serenity::ModalInteraction,
    data: &Bot,
) -> Result<(), Error> {
    let locale = locale_of(data, modal.guild_id, &modal.user.id);
    let t = i18n::get(locale);
    let mut modal_data = modal.data.clone();
    let email = poise::find_modal_text(&mut modal_data, INPUT_EMAIL).unwrap_or_default();
    let guild_id = modal
        .guild_id
        .ok_or_else(|| Error::Message("verification must happen inside a server".into()))?;
    let user_id = modal.user.id;

    match data
        .verify
        .start(&guild_id.to_string(), &user_id.to_string(), &email, locale)
        .await
    {
        Ok(()) => {
            let content = i18n::subst(t.code_sent_fmt, &[email.as_str()]);
            let row = CreateActionRow::Buttons(vec![
                CreateButton::new(BTN_ENTER_CODE)
                    .label(t.enter_code_btn)
                    .style(ButtonStyle::Success),
            ]);
            modal
                .create_response(
                    ctx,
                    CreateInteractionResponse::Message(
                        CreateInteractionResponseMessage::new()
                            .content(content)
                            .ephemeral(true)
                            .components(vec![row]),
                    ),
                )
                .await?;
        }
        Err(e) => {
            let localized = e.localize(t);
            // Parity: respondErr(t.ErrEmailFmt, localizeError) → "❌ Error: <msg>".
            let content = format!("❌ {}", i18n::subst(t.err_email_fmt, &[localized.as_str()]));
            modal
                .create_response(
                    ctx,
                    CreateInteractionResponse::Message(
                        CreateInteractionResponseMessage::new()
                            .content(content)
                            .ephemeral(true),
                    ),
                )
                .await?;
        }
    }
    Ok(())
}
