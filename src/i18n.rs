//! Bilingual (EN/CS) static translation system.
//!
//! Parity target: Go `internal/i18n/i18n.go`. All 114 `Translations` fields are
//! ported 1:1. Go format verbs (`%s`/`%d`) are converted to `{}` placeholders
//! and applied through [`subst`], the port of `fmt.Sprintf`.

/// Supported languages. Parity: Go `LocaleEN`/`LocaleCS` string constants.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum Locale {
    #[default]
    En,
    Cs,
}

impl Locale {
    /// Parity: Go `ParseLocale` — `strings.ToLower(strings.TrimSpace(s))`,
    /// accepting `cs`, `cz`, `cs-cz`, `cs_CZ`, `czech` (case-insensitive,
    /// whitespace-trimmed); everything else is English.
    pub fn parse(s: &str) -> Self {
        match s.trim().to_ascii_lowercase().as_str() {
            "cs" | "cz" | "cs-cz" | "cs_cz" | "czech" => Locale::Cs,
            _ => Locale::En,
        }
    }

    /// Stable code stored in `user_locales` ("en" / "cs").
    pub fn code(self) -> &'static str {
        match self {
            Locale::En => "en",
            Locale::Cs => "cs",
        }
    }
}

/// Port of Go `fmt.Sprintf` for the flat format-string fields. Replaces `{}`
/// placeholders sequentially; any remaining template tail (including unused
/// placeholders) is appended unchanged.
pub fn subst(template: &str, args: &[&str]) -> String {
    let mut out = String::with_capacity(template.len());
    let mut rest = template;
    let mut it = args.iter();
    while let Some(pos) = rest.find("{}") {
        match it.next() {
            Some(a) => {
                out.push_str(&rest[..pos]);
                out.push_str(a);
                rest = &rest[pos + 2..];
            }
            None => {
                out.push_str(rest);
                rest = "";
                break;
            }
        }
    }
    out.push_str(rest);
    out
}

/// One flat struct per locale — every user-facing string the bot emits.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Translations {
    pub setup_desc: &'static str,
    pub regex_desc: &'static str,
    pub csv_desc: &'static str,
    pub setup_domain: &'static str,
    pub setup_mode: &'static str,
    pub setup_channel: &'static str,
    pub setup_subject: &'static str,
    pub regex_add: &'static str,
    pub regex_list: &'static str,
    pub regex_remove: &'static str,
    pub regex_pattern: &'static str,
    pub regex_role: &'static str,
    pub regex_priority: &'static str,
    pub regex_id: &'static str,
    pub csv_upload: &'static str,
    pub csv_map: &'static str,
    pub csv_file: &'static str,
    pub csv_class: &'static str,
    pub csv_role: &'static str,
    pub language_desc: &'static str,
    pub language_set_fmt: &'static str,
    pub rate_limit_desc: &'static str,
    pub rate_limit_count_desc: &'static str,
    pub rate_limit_window_desc: &'static str,
    pub rate_limit_set_fmt: &'static str,
    pub verified_role_desc: &'static str,
    pub verified_role_set: &'static str,
    pub verified_role_view: &'static str,
    pub verified_role_clear: &'static str,
    pub verified_role_role: &'static str,
    pub verified_role_set_fmt: &'static str,
    pub verified_role_view_fmt: &'static str,
    pub verified_role_cleared: &'static str,
    pub verified_role_not_set: &'static str,
    pub help_desc: &'static str,
    pub help_text: &'static str,
    pub help_admin_title: &'static str,
    pub help_user_title: &'static str,
    pub help_click_hint: &'static str,

    pub verify_btn: &'static str,
    pub enter_code_btn: &'static str,
    pub verify_modal_title: &'static str,
    pub your_email: &'static str,
    pub email_placeholder: &'static str,
    pub code_modal_title: &'static str,
    pub code_label: &'static str,
    pub code_placeholder: &'static str,

    pub embed_title: &'static str,
    pub embed_desc_fmt: &'static str,

    pub code_sent_fmt: &'static str,
    pub verify_success: &'static str,
    pub config_saved: &'static str,
    pub config_saved_err: &'static str,
    pub rule_added: &'static str,
    pub no_rules: &'static str,
    pub rule_deleted: &'static str,
    pub failed_save: &'static str,
    pub failed_load_rules: &'static str,
    pub failed_delete: &'static str,
    pub failed_map: &'static str,
    pub uploaded_emails_fmt: &'static str,
    pub invalid_csv: &'static str,
    pub error_download: &'static str,
    pub class_mapped_fmt: &'static str,

    pub err_not_active: &'static str,
    pub err_rate_limited: &'static str,
    pub err_no_pending: &'static str,
    pub err_expired: &'static str,
    pub err_too_many_attempts: &'static str,
    pub err_send_failed: &'static str,
    pub err_email_already_used: &'static str,
    pub err_invalid_domain: &'static str,
    pub err_missing_config: &'static str,
    pub err_wrong_code_fmt: &'static str,
    pub err_email_fmt: &'static str,
    pub err_send_fmt: &'static str,

    pub email_hello: &'static str,
    pub email_code_for: &'static str,
    pub email_copy_btn: &'static str,
    pub email_valid_for_fmt: &'static str,
    pub email_sender_fallback: &'static str,
    pub default_subject: &'static str,
    pub html_title: &'static str,

    pub backup_desc: &'static str,
    pub backup_create: &'static str,
    pub backup_restore: &'static str,
    pub backup_list: &'static str,
    pub backup_schedule: &'static str,
    pub backup_schedule_off: &'static str,
    pub backup_delete: &'static str,
    pub backup_scope: &'static str,
    pub backup_scope_single: &'static str,
    pub backup_scope_multi: &'static str,
    pub backup_guild_id: &'static str,
    pub backup_freq: &'static str,
    pub backup_time_of_day: &'static str,
    pub backup_created_fmt: &'static str,
    pub backup_restored_fmt: &'static str,
    pub backup_no_backups: &'static str,
    pub backup_deleted_fmt: &'static str,
    pub backup_scheduled_fmt: &'static str,
    pub backup_scheduled_off: &'static str,
    pub backup_schedule_disabled: &'static str,
    pub backup_error_capture: &'static str,
    pub backup_error_restore: &'static str,
    pub backup_error_save: &'static str,
    pub backup_error_list: &'static str,
    pub backup_error_delete: &'static str,
    pub backup_confirm_restore: &'static str,
    pub backup_confirm_delete: &'static str,
    pub backup_next_run_fmt: &'static str,
    pub backup_manual_label: &'static str,
    pub backup_scheduled_label: &'static str,
    pub backup_all_label: &'static str,
}

/// Locale-aware string lookup. Parity: Go `Get` with English fallback.
pub fn get(locale: Locale) -> Translations {
    match locale {
        Locale::En => EN,
        Locale::Cs => CS,
    }
}

static EN: Translations = Translations {
    setup_desc: "Configure verification parameters for the server",
    regex_desc: "Manage Regex rules",
    csv_desc: "Manage CSV data",
    setup_domain: "Allowed email domain (e.g. sspu-opava.cz)",
    setup_mode: "Verification mode",
    setup_channel: "Verification channel",
    setup_subject: "Email subject",
    regex_add: "Add a regex rule",
    regex_list: "List all rules",
    regex_remove: "Remove a rule",
    regex_pattern: "Regex pattern",
    regex_role: "Target role",
    regex_priority: "Priority (higher = more important)",
    regex_id: "Rule ID",
    csv_upload: "Upload a CSV file (email,class)",
    csv_map: "Map a class to a role",
    csv_file: "CSV file",
    csv_class: "Class name from CSV",
    csv_role: "Discord role",
    language_desc: "Language",
    language_set_fmt: "Language set to {}.",
    rate_limit_desc: "Set email rate limits",
    rate_limit_count_desc: "Max emails (1-3)",
    rate_limit_window_desc: "Window in minutes (1-60)",
    rate_limit_set_fmt: "Rate limit set to {}",
    verified_role_desc: "Set the default role assigned to every verified user",
    verified_role_set: "Set the default verified role",
    verified_role_view: "Show the current default verified role",
    verified_role_clear: "Clear the default verified role",
    verified_role_role: "Discord role",
    verified_role_set_fmt: "Default verified role set to <@&{}>.",
    verified_role_view_fmt: "Default verified role: <@&{}>",
    verified_role_cleared: "Default verified role cleared.",
    verified_role_not_set: "No default verified role is currently set.",
    help_desc: "Show all commands",
    help_text: "Available Commands",
    help_admin_title: "Administrator",
    help_user_title: "User",
    help_click_hint: "Tip: Type / to see all commands in Discord",

    verify_btn: "Verify",
    enter_code_btn: "Enter Code",
    verify_modal_title: "School Email Verification",
    your_email: "Your email",
    email_placeholder: "student@domain.com",
    code_modal_title: "Enter code from email",
    code_label: "Verification code",
    code_placeholder: "123456",

    embed_title: "School Email Verification",
    embed_desc_fmt: "To gain access, click the button and enter your school email (@{}).",

    code_sent_fmt: "Code sent to {}. Check your inbox and click the button below to enter it.",
    verify_success: "Verification successful! The role has been assigned.",
    config_saved: "Server successfully configured.",
    config_saved_err: "Configuration saved, but failed to send the message to the channel.",
    rule_added: "Rule added.",
    no_rules: "No rules are set.",
    rule_deleted: "Rule deleted.",
    failed_save: "Failed to save configuration.",
    failed_load_rules: "Failed to load rules.",
    failed_delete: "Failed to delete rule.",
    failed_map: "Failed to save mapping.",
    uploaded_emails_fmt: "Uploaded {} emails into the database.",
    invalid_csv: "Invalid CSV format.",
    error_download: "Error downloading file.",
    class_mapped_fmt: "Class `{}` mapped to role <@&{}>.",

    err_not_active: "No rule matches this email",
    err_rate_limited: "Verification limit exceeded, please try again later",
    err_no_pending: "No pending code found, please use /verify first",
    err_expired: "Code expired",
    err_too_many_attempts: "Too many attempts",
    err_send_failed: "Failed to send email",
    err_email_already_used: "Email is already bound to another user",
    err_invalid_domain: "Invalid email domain for this server",
    err_missing_config: "This server is not fully configured yet",
    err_wrong_code_fmt: "Wrong code. {} attempts remaining.",
    err_email_fmt: "Error: {}",
    err_send_fmt: "Verification failed: {}",

    email_hello: "Hello,",
    email_code_for: "Your verification code for Discord is:",
    email_copy_btn: "COPY",
    email_valid_for_fmt: "The code is valid for {} minutes. If you did not request this, please ignore this email.",
    email_sender_fallback: "Discord bot",
    default_subject: "Verification code",
    html_title: "Discord Verification",

    backup_desc: "Backup and restore server structure",
    backup_create: "Create a backup",
    backup_restore: "Restore a backup",
    backup_list: "List backups",
    backup_schedule: "Configure scheduled backups",
    backup_schedule_off: "Disable scheduled backups",
    backup_delete: "Delete a backup",
    backup_scope: "Backup scope",
    backup_scope_single: "Single server (current guild)",
    backup_scope_multi: "Multi-server (requires guild ID)",
    backup_guild_id: "Target guild ID",
    backup_freq: "Frequency",
    backup_time_of_day: "Time of day (HH:MM)",
    backup_created_fmt: "Backup created: {}",
    backup_restored_fmt: "Backup restored: {}",
    backup_no_backups: "No backups found.",
    backup_deleted_fmt: "Backup deleted: {}",
    backup_scheduled_fmt: "Scheduled backups enabled: {}",
    backup_scheduled_off: "Scheduled backups disabled.",
    backup_schedule_disabled: "Scheduled backups are disabled.",
    backup_error_capture: "Failed to capture server structure.",
    backup_error_restore: "Failed to restore server structure.",
    backup_error_save: "Failed to save backup record.",
    backup_error_list: "Failed to list backups.",
    backup_error_delete: "Failed to delete backup.",
    backup_confirm_restore: "Restore this backup? This will create missing channels, roles, and apply bans.",
    backup_confirm_delete: "Delete this backup permanently?",
    backup_next_run_fmt: "Next scheduled backup: {}",
    backup_manual_label: "Manual",
    backup_scheduled_label: "Scheduled",
    backup_all_label: "All",
};

static CS: Translations = Translations {
    setup_desc: "Nastavení parametrů ověření pro server",
    regex_desc: "Správa Regex pravidel",
    csv_desc: "Správa CSV dat",
    setup_domain: "Povolená emailová doména (např. sspu-opava.cz)",
    setup_mode: "Režim ověření",
    setup_channel: "Ověrový kanál",
    setup_subject: "Předmět e-mailu",
    regex_add: "Přidat regex pravidlo",
    regex_list: "Zobrazit všechna pravidla",
    regex_remove: "Odebrat pravidlo",
    regex_pattern: "Regex vzor",
    regex_role: "Cílová role",
    regex_priority: "Priorita (vyšší = důležitější)",
    regex_id: "ID pravidla",
    csv_upload: "Nahrát CSV soubor (email,trida)",
    csv_map: "Namapovat třídu na roli",
    csv_file: "CSV soubor",
    csv_class: "Název třídy z CSV",
    csv_role: "Discord role",
    language_desc: "Jazyk",
    language_set_fmt: "Jazyk nastaven na {}.",
    rate_limit_desc: "Nastavit limity emailů",
    rate_limit_count_desc: "Max emailů (1-3)",
    rate_limit_window_desc: "Časové okno v minutách (1-60)",
    rate_limit_set_fmt: "Limit nastaven na {}",
    verified_role_desc: "Nastavit výchozí roli přiřazenou každému ověřenému uživateli",
    verified_role_set: "Nastavit výchozí roli pro ověřené",
    verified_role_view: "Zobrazit aktuální výchozí roli pro ověřené",
    verified_role_clear: "Odebrat výchozí roli pro ověřené",
    verified_role_role: "Discord role",
    verified_role_set_fmt: "Výchozí role pro ověřené nastavena na <@&{}>.",
    verified_role_view_fmt: "Výchozí role pro ověřené: <@&{}>",
    verified_role_cleared: "Výchozí role pro ověřené odebrána.",
    verified_role_not_set: "Výchozí role pro ověřené není momentálně nastavena.",
    help_desc: "Zobrazit všechny příkazy",
    help_text: "Dostupné příkazy",
    help_admin_title: "Administrátor",
    help_user_title: "Uživatel",
    help_click_hint: "Tip: Zadej / pro zobrazení všech příkazů v Discordu",

    verify_btn: "Ověřit",
    enter_code_btn: "Zadat kód",
    verify_modal_title: "Ověření školní emailové adresy",
    your_email: "Tvůj email",
    email_placeholder: "student@domena.cz",
    code_modal_title: "Zadejte kód z e-mailu",
    code_label: "Ověrový kód",
    code_placeholder: "123456",

    embed_title: "Ověření školní emailové adresy",
    embed_desc_fmt: "Pro přístup klikněte na tlačítko a zadejte svou školní emailovou adresu (@{}).",

    code_sent_fmt: "Kód byl odeslán na {}. Zkontrolujte schránku a klikněte na tlačítko níže pro zadání kódu.",
    verify_success: "Ověření úspěšné! Role byla přiřazena.",
    config_saved: "Server byl úspěšně nakonfigurován.",
    config_saved_err: "Konfigurace uložena, ale nelze odeslat zprávu do kanálu.",
    rule_added: "Pravidlo přidáno.",
    no_rules: "Žádná pravidla nejsou nastavena.",
    rule_deleted: "Pravidlo odstraněno.",
    failed_save: "Nepodařilo se uložit konfiguraci.",
    failed_load_rules: "Nepodařilo se načíst pravidla.",
    failed_delete: "Nepodařilo se smazat pravidlo.",
    failed_map: "Nepodařilo se uložit mapování.",
    uploaded_emails_fmt: "Nahráno {} emailů do databáze.",
    invalid_csv: "Neplatný formát CSV.",
    error_download: "Chyba při stahování souboru.",
    class_mapped_fmt: "Třída `{}` mapována na roli <@&{}>.",

    err_not_active: "Žádné pravidlo neodpovídá této emailové adrese",
    err_rate_limited: "Překročen limit ověření, zkuste to prosím později",
    err_no_pending: "Nebyl nalezen žádný čekající kód, použijte /verify nejprve",
    err_expired: "Kód vypršel",
    err_too_many_attempts: "Příliš mnoho pokusů",
    err_send_failed: "Nepodařilo se odeslat e-mail",
    err_email_already_used: "Email je již přiřazen jinému uživateli",
    err_invalid_domain: "Neplatná emailová doména pro tento server",
    err_missing_config: "Tento server není plně nakonfigurován",
    err_wrong_code_fmt: "Špatný kód. Zbývá {} pokusů.",
    err_email_fmt: "Chyba: {}",
    err_send_fmt: "Ověření selhalo: {}",

    email_hello: "Dobrý den,",
    email_code_for: "Váš ověřovací kód pro Discord je:",
    email_copy_btn: "ZKOPÍROVAT",
    email_valid_for_fmt: "Kód je platný {} minut. Pokud jste o něj nežádali, ignorujte tento e-mail.",
    email_sender_fallback: "Discord bot",
    default_subject: "Ověřovací kód",
    html_title: "Ověření Discordem",

    backup_desc: "Zálohování a obnova struktury serveru",
    backup_create: "Vytvořit zálohu",
    backup_restore: "Obnovit ze zálohy",
    backup_list: "Zobrazit zálohy",
    backup_schedule: "Nastavit plánované zálohy",
    backup_schedule_off: "Vypnout plánované zálohy",
    backup_delete: "Smazat zálohu",
    backup_scope: "Rozsah zálohy",
    backup_scope_single: "Jeden server (aktuální guild)",
    backup_scope_multi: "Více serverů (vyžaduje ID guildu)",
    backup_guild_id: "Cílové guild ID",
    backup_freq: "Frekvence",
    backup_time_of_day: "Čas v den (HH:MM)",
    backup_created_fmt: "Záloha vytvořena: {}",
    backup_restored_fmt: "Záloha obnovena: {}",
    backup_no_backups: "Žádné zálohy neexistují.",
    backup_deleted_fmt: "Záloha smazána: {}",
    backup_scheduled_fmt: "Plánované zálohy zapnuty: {}",
    backup_scheduled_off: "Plánované zálohy vypnuty.",
    backup_schedule_disabled: "Plánované zálohy jsou vypnuty.",
    backup_error_capture: "Selhalo načtení struktury serveru.",
    backup_error_restore: "Selhalo obnovení struktury serveru.",
    backup_error_save: "Selhalo uložení záznamu zálohy.",
    backup_error_list: "Selhalo načtení seznamu záloh.",
    backup_error_delete: "Selhalo smazání zálohy.",
    backup_confirm_restore: "Obnovit tuto zálohu? Vytvoří chybějící kanály, role a použije bany.",
    backup_confirm_delete: "Trvale smazat tuto zálohu?",
    backup_next_run_fmt: "Další plánovaná záloha: {}",
    backup_manual_label: "Manuální",
    backup_scheduled_label: "Plánované",
    backup_all_label: "Vše",
};

#[cfg(test)]
mod tests {
    use super::*;

    fn all_fields(t: &Translations) -> Vec<&'static str> {
        vec![
            t.setup_desc,
            t.regex_desc,
            t.csv_desc,
            t.setup_domain,
            t.setup_mode,
            t.setup_channel,
            t.setup_subject,
            t.regex_add,
            t.regex_list,
            t.regex_remove,
            t.regex_pattern,
            t.regex_role,
            t.regex_priority,
            t.regex_id,
            t.csv_upload,
            t.csv_map,
            t.csv_file,
            t.csv_class,
            t.csv_role,
            t.language_desc,
            t.language_set_fmt,
            t.rate_limit_desc,
            t.rate_limit_count_desc,
            t.rate_limit_window_desc,
            t.rate_limit_set_fmt,
            t.verified_role_desc,
            t.verified_role_set,
            t.verified_role_view,
            t.verified_role_clear,
            t.verified_role_role,
            t.verified_role_set_fmt,
            t.verified_role_view_fmt,
            t.verified_role_cleared,
            t.verified_role_not_set,
            t.help_desc,
            t.help_text,
            t.help_admin_title,
            t.help_user_title,
            t.help_click_hint,
            t.verify_btn,
            t.enter_code_btn,
            t.verify_modal_title,
            t.your_email,
            t.email_placeholder,
            t.code_modal_title,
            t.code_label,
            t.code_placeholder,
            t.embed_title,
            t.embed_desc_fmt,
            t.code_sent_fmt,
            t.verify_success,
            t.config_saved,
            t.config_saved_err,
            t.rule_added,
            t.no_rules,
            t.rule_deleted,
            t.failed_save,
            t.failed_load_rules,
            t.failed_delete,
            t.failed_map,
            t.uploaded_emails_fmt,
            t.invalid_csv,
            t.error_download,
            t.class_mapped_fmt,
            t.err_not_active,
            t.err_rate_limited,
            t.err_no_pending,
            t.err_expired,
            t.err_too_many_attempts,
            t.err_send_failed,
            t.err_email_already_used,
            t.err_invalid_domain,
            t.err_missing_config,
            t.err_wrong_code_fmt,
            t.err_email_fmt,
            t.err_send_fmt,
            t.email_hello,
            t.email_code_for,
            t.email_copy_btn,
            t.email_valid_for_fmt,
            t.email_sender_fallback,
            t.default_subject,
            t.html_title,
            t.backup_desc,
            t.backup_create,
            t.backup_restore,
            t.backup_list,
            t.backup_schedule,
            t.backup_schedule_off,
            t.backup_delete,
            t.backup_scope,
            t.backup_scope_single,
            t.backup_scope_multi,
            t.backup_guild_id,
            t.backup_freq,
            t.backup_time_of_day,
            t.backup_created_fmt,
            t.backup_restored_fmt,
            t.backup_no_backups,
            t.backup_deleted_fmt,
            t.backup_scheduled_fmt,
            t.backup_scheduled_off,
            t.backup_schedule_disabled,
            t.backup_error_capture,
            t.backup_error_restore,
            t.backup_error_save,
            t.backup_error_list,
            t.backup_error_delete,
            t.backup_confirm_restore,
            t.backup_confirm_delete,
            t.backup_next_run_fmt,
            t.backup_manual_label,
            t.backup_scheduled_label,
            t.backup_all_label,
        ]
    }

    #[test]
    fn all_fields_non_empty_in_both_locales() {
        for f in all_fields(&get(Locale::En)) {
            assert!(!f.is_empty(), "EN field empty");
        }
        for f in all_fields(&get(Locale::Cs)) {
            assert!(!f.is_empty(), "CS field empty");
        }
    }

    #[test]
    fn parse_accepts_czech_aliases() {
        for s in [
            "cs", "cz", "cs-cz", "cs_CZ", "czech", "  Cs ", "CZ", "Czech", "CS-CZ",
        ] {
            assert_eq!(Locale::parse(s), Locale::Cs, "input {s:?}");
        }
    }

    #[test]
    fn parse_defaults_to_english() {
        for s in ["en", "de", "xx", "fr", "", "english", "csx"] {
            assert_eq!(Locale::parse(s), Locale::En, "input {s:?}");
        }
    }

    #[test]
    fn code_round_trip() {
        assert_eq!(Locale::En.code(), "en");
        assert_eq!(Locale::Cs.code(), "cs");
        assert_eq!(Locale::parse(Locale::Cs.code()), Locale::Cs);
        assert_eq!(Locale::parse(Locale::En.code()), Locale::En);
    }

    #[test]
    fn subst_one_arg() {
        assert_eq!(
            subst("Wrong code. {} attempts.", &["3"]),
            "Wrong code. 3 attempts."
        );
    }

    #[test]
    fn subst_two_args() {
        assert_eq!(
            subst("Class `{}` mapped to role <@&{}>.", &["3A", "123"]),
            "Class `3A` mapped to role <@&123>."
        );
    }

    #[test]
    fn subst_no_args() {
        assert_eq!(subst("plain", &[]), "plain");
        assert_eq!(subst("has {} but none", &[]), "has {} but none");
    }

    #[test]
    fn subst_value_containing_placeholder() {
        assert_eq!(subst("code: {}", &["a{}b"]), "code: a{}b");
    }
}
