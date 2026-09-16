package i18n

import "strings"

type Locale string

const (
	LocaleEN Locale = "en"
	LocaleCS Locale = "cs"
)

func ParseLocale(s string) Locale {
	switch strings.ToLower(strings.TrimSpace(s)) {
	case "cs", "cz", "cs-cz", "cs_CZ", "czech":
		return LocaleCS
	default:
		return LocaleEN
	}
}

type Translations struct {
	SetupDesc        string
	RegexDesc        string
	CsvDesc          string
	SetupDomain      string
	SetupMode        string
	SetupChannel     string
	SetupSubject     string
	RegexAdd         string
	RegexList        string
	RegexRemove      string
	RegexPattern     string
	RegexRole        string
	RegexPriority    string
	RegexID          string
	CsvUpload        string
	CsvMap           string
	CsvFile          string
	CsvClass         string
	CsvRole          string
	LanguageDesc     string
	LanguageSetFmt   string
	RateLimitDesc    string
	RateLimitCountDesc string
	RateLimitWindowDesc string
	RateLimitSetFmt  string
	VerifiedRoleDesc string
	VerifiedRoleSet  string
	VerifiedRoleView string
	VerifiedRoleClear string
	VerifiedRoleRole string
	VerifiedRoleSetFmt string
	VerifiedRoleViewFmt string
	VerifiedRoleCleared string
	VerifiedRoleNotSet string
	HelpDesc         string
	HelpText         string
	HelpAdminTitle   string
	HelpUserTitle    string
	HelpClickHint    string

	VerifyBtn        string
	EnterCodeBtn     string
	VerifyModalTitle string
	YourEmail        string
	EmailPlaceholder string
	CodeModalTitle   string
	CodeLabel        string
	CodePlaceholder  string

	EmbedTitle   string
	EmbedDescFmt string

	CodeSentFmt       string
	VerifySuccess     string
	ConfigSaved       string
	ConfigSavedErr    string
	RuleAdded         string
	NoRules           string
	RuleDeleted       string
	FailedSave        string
	FailedLoadRules   string
	FailedDelete      string
	FailedMap         string
	UploadedEmailsFmt string
	InvalidCSV        string
	ErrorDownload     string
	ClassMappedFmt    string

	ErrNotActive       string
	ErrRateLimited     string
	ErrNoPending       string
	ErrExpired         string
	ErrTooManyAttempts string
	ErrSendFailed      string
	ErrEmailAlreadyUsed string
	ErrInvalidDomain   string
	ErrMissingConfig   string
	ErrWrongCodeFmt    string
	ErrEmailFmt        string
	ErrSendFmt         string

	EmailHello        string
	EmailCodeFor      string
	EmailCopyBtn      string
	EmailValidForFmt  string
	EmailSenderFallback string
	DefaultSubject    string
	HTMLTitle         string

	// Backup
	BackupDesc            string
	BackupCreate          string
	BackupRestore         string
	BackupList            string
	BackupSchedule        string
	BackupScheduleOff     string
	BackupDelete          string
	BackupScope           string
	BackupScopeSingle     string
	BackupScopeMulti      string
	BackupGuildID         string
	BackupFreq            string
	BackupTimeOfDay       string
	BackupCreatedFmt      string
	BackupRestoredFmt     string
	BackupNoBackups       string
	BackupDeletedFmt      string
	BackupScheduledFmt     string
	BackupScheduledOff     string
	BackupScheduleDisabled string
	BackupErrorCapture    string
	BackupErrorRestore    string
	BackupErrorSave       string
	BackupErrorList       string
	BackupErrorDelete     string
	BackupConfirmRestore  string
	BackupConfirmDelete   string
	BackupNextRunFmt      string
	BackupManualLabel     string
	BackupScheduledLabel  string
	BackupAllLabel        string
}

var en = Translations{
	SetupDesc:        "Configure verification parameters for the server",
	RegexDesc:        "Manage Regex rules",
	CsvDesc:          "Manage CSV data",
	SetupDomain:      "Allowed email domain (e.g. sspu-opava.cz)",
	SetupMode:        "Verification mode",
	SetupChannel:     "Verification channel",
	SetupSubject:     "Email subject",
	RegexAdd:         "Add a regex rule",
	RegexList:        "List all rules",
	RegexRemove:      "Remove a rule",
	RegexPattern:     "Regex pattern",
	RegexRole:        "Target role",
	RegexPriority:    "Priority (higher = more important)",
	RegexID:          "Rule ID",
	CsvUpload:        "Upload a CSV file (email,class)",
	CsvMap:           "Map a class to a role",
	CsvFile:          "CSV file",
	CsvClass:         "Class name from CSV",
	CsvRole:          "Discord role",
	LanguageDesc:     "Language",
	LanguageSetFmt:   "Language set to %s.",
	RateLimitDesc:    "Set email rate limits",
	RateLimitCountDesc: "Max emails (1-3)",
	RateLimitWindowDesc: "Window in minutes (1-60)",
	RateLimitSetFmt:  "Rate limit set to %s",
	VerifiedRoleDesc: "Set the default role assigned to every verified user",
	VerifiedRoleSet:  "Set the default verified role",
	VerifiedRoleView: "Show the current default verified role",
	VerifiedRoleClear: "Clear the default verified role",
	VerifiedRoleRole: "Discord role",
	VerifiedRoleSetFmt: "Default verified role set to <@&%s>.",
	VerifiedRoleViewFmt: "Default verified role: <@&%s>",
	VerifiedRoleCleared: "Default verified role cleared.",
	VerifiedRoleNotSet: "No default verified role is currently set.",
	HelpDesc:         "Show all commands",
	HelpText:         "Available Commands",
	HelpAdminTitle:   "Administrator",
	HelpUserTitle:    "User",
	HelpClickHint:    "Tip: Type / to see all commands in Discord",

	VerifyBtn:        "Verify",
	EnterCodeBtn:     "Enter Code",
	VerifyModalTitle: "School Email Verification",
	YourEmail:        "Your email",
	EmailPlaceholder: "student@domain.com",
	CodeModalTitle:   "Enter code from email",
	CodeLabel:        "Verification code",
	CodePlaceholder:  "123456",

	EmbedTitle:   "School Email Verification",
	EmbedDescFmt: "To gain access, click the button and enter your school email (@%s).",

	CodeSentFmt:       "Code sent to %s. Check your inbox and click the button below to enter it.",
	VerifySuccess:     "Verification successful! The role has been assigned.",
	ConfigSaved:       "Server successfully configured.",
	ConfigSavedErr:    "Configuration saved, but failed to send the message to the channel.",
	RuleAdded:         "Rule added.",
	NoRules:           "No rules are set.",
	RuleDeleted:       "Rule deleted.",
	FailedSave:        "Failed to save configuration.",
	FailedLoadRules:   "Failed to load rules.",
	FailedDelete:      "Failed to delete rule.",
	FailedMap:         "Failed to save mapping.",
	UploadedEmailsFmt: "Uploaded %d emails into the database.",
	InvalidCSV:        "Invalid CSV format.",
	ErrorDownload:     "Error downloading file.",
	ClassMappedFmt:    "Class `%s` mapped to role <@&%s>.",

	ErrNotActive:        "No rule matches this email",
	ErrRateLimited:      "Verification limit exceeded, please try again later",
	ErrNoPending:        "No pending code found, please use /verify first",
	ErrExpired:          "Code expired",
	ErrTooManyAttempts:  "Too many attempts",
	ErrSendFailed:       "Failed to send email",
	ErrEmailAlreadyUsed: "Email is already bound to another user",
	ErrInvalidDomain:    "Invalid email domain for this server",
	ErrMissingConfig:    "This server is not fully configured yet",
	ErrWrongCodeFmt:     "Wrong code. %d attempts remaining.",
	ErrEmailFmt:         "Error: %s",
	ErrSendFmt:          "Verification failed: %s",

	EmailHello:         "Hello,",
	EmailCodeFor:       "Your verification code for Discord is:",
	EmailCopyBtn:       "COPY",
	EmailValidForFmt:   "The code is valid for %d minutes. If you did not request this, please ignore this email.",
	EmailSenderFallback: "Discord bot",
	DefaultSubject:     "Verification code",
	HTMLTitle:          "Discord Verification",

	// Backup
	BackupDesc:            "Backup and restore server structure",
	BackupCreate:          "Create a backup",
	BackupRestore:         "Restore a backup",
	BackupList:            "List backups",
	BackupSchedule:        "Configure scheduled backups",
	BackupScheduleOff:     "Disable scheduled backups",
	BackupDelete:          "Delete a backup",
	BackupScope:           "Backup scope",
	BackupScopeSingle:     "Single server (current guild)",
	BackupScopeMulti:      "Multi-server (requires guild ID)",
	BackupGuildID:         "Target guild ID",
	BackupFreq:            "Frequency",
	BackupTimeOfDay:       "Time of day (HH:MM)",
	BackupCreatedFmt:      "Backup created: %s",
	BackupRestoredFmt:     "Backup restored: %s",
	BackupNoBackups:       "No backups found.",
	BackupDeletedFmt:      "Backup deleted: %s",
	BackupScheduledFmt:    "Scheduled backups enabled: %s",
	BackupScheduledOff:     "Scheduled backups disabled.",
	BackupScheduleDisabled: "Scheduled backups are disabled.",
	BackupErrorCapture:    "Failed to capture server structure.",
	BackupErrorRestore:    "Failed to restore server structure.",
	BackupErrorSave:       "Failed to save backup record.",
	BackupErrorList:       "Failed to list backups.",
	BackupErrorDelete:     "Failed to delete backup.",
	BackupConfirmRestore:  "Restore this backup? This will create missing channels, roles, and apply bans.",
	BackupConfirmDelete:   "Delete this backup permanently?",
	BackupNextRunFmt:      "Next scheduled backup: %s",
	BackupManualLabel:     "Manual",
	BackupScheduledLabel:  "Scheduled",
	BackupAllLabel:        "All",
}

var cs = Translations{
	SetupDesc:        "Nastavení parametrů ověření pro server",
	RegexDesc:        "Správa Regex pravidel",
	CsvDesc:          "Správa CSV dat",
	SetupDomain:      "Povolená emailová doména (např. sspu-opava.cz)",
	SetupMode:        "Režim ověření",
	SetupChannel:     "Ověrový kanál",
	SetupSubject:     "Předmět e-mailu",
	RegexAdd:         "Přidat regex pravidlo",
	RegexList:        "Zobrazit všechna pravidla",
	RegexRemove:      "Odebrat pravidlo",
	RegexPattern:     "Regex vzor",
	RegexRole:        "Cílová role",
	RegexPriority:    "Priorita (vyšší = důležitější)",
	RegexID:          "ID pravidla",
	CsvUpload:        "Nahrát CSV soubor (email,trida)",
	CsvMap:           "Namapovat třídu na roli",
	CsvFile:          "CSV soubor",
	CsvClass:         "Název třídy z CSV",
	CsvRole:          "Discord role",
	LanguageDesc:     "Jazyk",
	LanguageSetFmt:   "Jazyk nastaven na %s.",
	RateLimitDesc:    "Nastavit limity emailů",
	RateLimitCountDesc: "Max emailů (1-3)",
	RateLimitWindowDesc: "Časové okno v minutách (1-60)",
	RateLimitSetFmt:  "Limit nastaven na %s",
	VerifiedRoleDesc: "Nastavit výchozí roli přiřazenou každému ověřenému uživateli",
	VerifiedRoleSet:  "Nastavit výchozí roli pro ověřené",
	VerifiedRoleView: "Zobrazit aktuální výchozí roli pro ověřené",
	VerifiedRoleClear: "Odebrat výchozí roli pro ověřené",
	VerifiedRoleRole: "Discord role",
	VerifiedRoleSetFmt: "Výchozí role pro ověřené nastavena na <@&%s>.",
	VerifiedRoleViewFmt: "Výchozí role pro ověřené: <@&%s>",
	VerifiedRoleCleared: "Výchozí role pro ověřené odebrána.",
	VerifiedRoleNotSet: "Výchozí role pro ověřené není momentálně nastavena.",
	HelpDesc:         "Zobrazit všechny příkazy",
	HelpText:         "Dostupné příkazy",
	HelpAdminTitle:   "Administrátor",
	HelpUserTitle:    "Uživatel",
	HelpClickHint:    "Tip: Zadej / pro zobrazení všech příkazů v Discordu",

	VerifyBtn:        "Ověřit",
	EnterCodeBtn:     "Zadat kód",
	VerifyModalTitle: "Ověření školní emailové adresy",
	YourEmail:        "Tvůj email",
	EmailPlaceholder: "student@domena.cz",
	CodeModalTitle:   "Zadejte kód z e-mailu",
	CodeLabel:        "Ověrový kód",
	CodePlaceholder:  "123456",

	EmbedTitle:   "Ověření školní emailové adresy",
	EmbedDescFmt: "Pro přístup klikněte na tlačítko a zadejte svou školní emailovou adresu (@%s).",

	CodeSentFmt:       "Kód byl odeslán na %s. Zkontrolujte schránku a klikněte na tlačítko níže pro zadání kódu.",
	VerifySuccess:     "Ověření úspěšné! Role byla přiřazena.",
	ConfigSaved:       "Server byl úspěšně nakonfigurován.",
	ConfigSavedErr:    "Konfigurace uložena, ale nelze odeslat zprávu do kanálu.",
	RuleAdded:         "Pravidlo přidáno.",
	NoRules:           "Žádná pravidla nejsou nastavena.",
	RuleDeleted:       "Pravidlo odstraněno.",
	FailedSave:        "Nepodařilo se uložit konfiguraci.",
	FailedLoadRules:   "Nepodařilo se načíst pravidla.",
	FailedDelete:      "Nepodařilo se smazat pravidlo.",
	FailedMap:         "Nepodařilo se uložit mapování.",
	UploadedEmailsFmt: "Nahráno %d emailů do databáze.",
	InvalidCSV:        "Neplatný formát CSV.",
	ErrorDownload:     "Chyba při stahování souboru.",
	ClassMappedFmt:    "Třída `%s` mapována na roli <@&%s>.",

	ErrNotActive:        "Žádné pravidlo neodpovídá této emailové adrese",
	ErrRateLimited:      "Překročen limit ověření, zkuste to prosím později",
	ErrNoPending:        "Nebyl nalezen žádný čekající kód, použijte /verify nejprve",
	ErrExpired:          "Kód vypršel",
	ErrTooManyAttempts:  "Příliš mnoho pokusů",
	ErrSendFailed:       "Nepodařilo se odeslat e-mail",
	ErrEmailAlreadyUsed: "Email je již přiřazen jinému uživateli",
	ErrInvalidDomain:    "Neplatná emailová doména pro tento server",
	ErrMissingConfig:    "Tento server není plně nakonfigurován",
	ErrWrongCodeFmt:     "Špatný kód. Zbývá %d pokusů.",
	ErrEmailFmt:         "Chyba: %s",
	ErrSendFmt:          "Ověření selhalo: %s",

	EmailHello:          "Dobrý den,",
	EmailCodeFor:        "Váš ověřovací kód pro Discord je:",
	EmailCopyBtn:        "ZKOPÍROVAT",
	EmailValidForFmt:    "Kód je platný %d minut. Pokud jste o něj nežádali, ignorujte tento e-mail.",
	EmailSenderFallback: "Discord bot",
	DefaultSubject:      "Ověřovací kód",
	HTMLTitle:           "Ověření Discordem",

	// Backup
	BackupDesc:            "Zálohování a obnova struktury serveru",
	BackupCreate:          "Vytvořit zálohu",
	BackupRestore:         "Obnovit ze zálohy",
	BackupList:            "Zobrazit zálohy",
	BackupSchedule:        "Nastavit plánované zálohy",
	BackupScheduleOff:     "Vypnout plánované zálohy",
	BackupDelete:          "Smazat zálohu",
	BackupScope:           "Rozsah zálohy",
	BackupScopeSingle:     "Jeden server (aktuální guild)",
	BackupScopeMulti:      "Více serverů (vyžaduje ID guildu)",
	BackupGuildID:         "Cílové guild ID",
	BackupFreq:            "Frekvence",
	BackupTimeOfDay:       "Čas v den (HH:MM)",
	BackupCreatedFmt:      "Záloha vytvořena: %s",
	BackupRestoredFmt:     "Záloha obnovena: %s",
	BackupNoBackups:       "Žádné zálohy neexistují.",
	BackupDeletedFmt:      "Záloha smazána: %s",
	BackupScheduledFmt:    "Plánované zálohy zapnuty: %s",
	BackupScheduledOff:     "Plánované zálohy vypnuty.",
	BackupScheduleDisabled: "Plánované zálohy jsou vypnuty.",
	BackupErrorCapture:    "Selhalo načtení struktury serveru.",
	BackupErrorRestore:    "Selhalo obnovení struktury serveru.",
	BackupErrorSave:       "Selhalo uložení záznamu zálohy.",
	BackupErrorList:       "Selhalo načtení seznamu záloh.",
	BackupErrorDelete:     "Selhalo smazání zálohy.",
	BackupConfirmRestore:  "Obnovit tuto zálohu? Vytvoří chybějící kanály, role a použije bany.",
	BackupConfirmDelete:   "Trvale smazat tuto zálohu?",
	BackupNextRunFmt:      "Další plánovaná záloha: %s",
	BackupManualLabel:     "Manuální",
	BackupScheduledLabel:  "Plánované",
	BackupAllLabel:        "Vše",
}

var translations = map[Locale]Translations{
	LocaleEN: en,
	LocaleCS: cs,
}

func Get(locale Locale) Translations {
	if t, ok := translations[locale]; ok {
		return t
	}
	return en
}
