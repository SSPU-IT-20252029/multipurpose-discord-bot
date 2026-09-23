package main

import (
	"context"
	"encoding/csv"
	"errors"
	"flag"
	"fmt"
	"io"
	"log"
	"net/http"
	"os"
	"os/signal"
	"path/filepath"
	"strings"
	"syscall"
	"time"

	"github.com/bwmarrin/discordgo"

	"sspu-multipurpose-discord-bot/internal/backup"
	"sspu-multipurpose-discord-bot/internal/config"
	"sspu-multipurpose-discord-bot/internal/i18n"
	"sspu-multipurpose-discord-bot/internal/mailer"
	"sspu-multipurpose-discord-bot/internal/store"
	"sspu-multipurpose-discord-bot/internal/verify"
)

func ptrBool(b bool) *bool {
	return &b
}

var configPath = flag.String("config", "config.yml", "Path to configuration file")
var debugMode = flag.Bool("debug", false, "Enable debug logging")

func getOptionByName(options []*discordgo.ApplicationCommandInteractionDataOption, name string) *discordgo.ApplicationCommandInteractionDataOption {
	for _, option := range options {
		if option != nil && option.Name == name {
			return option
		}
	}
	return nil
}

func getStringOption(options []*discordgo.ApplicationCommandInteractionDataOption, name string) (string, bool) {
	option := getOptionByName(options, name)
	if option == nil || option.Type != discordgo.ApplicationCommandOptionString || option.Value == nil {
		return "", false
	}
	value, ok := option.Value.(string)
	if !ok {
		return "", false
	}
	return value, true
}

func getIntOption(options []*discordgo.ApplicationCommandInteractionDataOption, name string) (int64, bool) {
	option := getOptionByName(options, name)
	if option == nil || option.Type != discordgo.ApplicationCommandOptionInteger || option.Value == nil {
		return 0, false
	}
	value, ok := option.Value.(float64)
	if !ok {
		return 0, false
	}
	return int64(value), true
}

func getRoleOption(options []*discordgo.ApplicationCommandInteractionDataOption, name string) (*discordgo.Role, bool) {
	option := getOptionByName(options, name)
	if option == nil || option.Type != discordgo.ApplicationCommandOptionRole || option.Value == nil {
		return nil, false
	}
	if _, ok := option.Value.(string); !ok {
		return nil, false
	}
	return option.RoleValue(nil, ""), true
}

func getAttachmentOption(options []*discordgo.ApplicationCommandInteractionDataOption, name string) (string, bool) {
	option := getOptionByName(options, name)
	if option == nil || option.Type != discordgo.ApplicationCommandOptionAttachment || option.Value == nil {
		return "", false
	}
	value, ok := option.Value.(string)
	if !ok {
		return "", false
	}
	return value, true
}

func getChannelOption(options []*discordgo.ApplicationCommandInteractionDataOption, name string) (string, bool) {
	option := getOptionByName(options, name)
	if option == nil || option.Type != discordgo.ApplicationCommandOptionChannel || option.Value == nil {
		return "", false
	}
	value, ok := option.Value.(string)
	if !ok {
		return "", false
	}
	return value, true
}

func getSubCommandOption(options []*discordgo.ApplicationCommandInteractionDataOption) (*discordgo.ApplicationCommandInteractionDataOption, bool) {
	for _, option := range options {
		if option.Type == discordgo.ApplicationCommandOptionSubCommand {
			return option, true
		}
	}
	return nil, false
}

type Bot struct {
	session *discordgo.Session
	store   *store.Store
	verify  *verify.Service
	backup  *backup.Scheduler
}

func main() {
	flag.Parse()

	if *debugMode {
		log.SetFlags(log.LstdFlags | log.Lshortfile)
		log.Println("Debug mode enabled")
	}

	cfg, err := config.Load(*configPath)
	if err != nil {
		log.Fatalf("Error loading config: %v", err)
	}

	st, err := store.Open(cfg.Storage.DSN)
	if err != nil {
		log.Fatalf("Error loading database: %v", err)
	}
	defer st.Close()

	m := mailer.New(cfg.Email)
	v := verify.New(st, m)

	dg, err := discordgo.New("Bot " + cfg.Discord.Token)
	if err != nil {
		log.Fatalf("Error creating Discord session: %v", err)
	}

	if *debugMode {
		dg.LogLevel = discordgo.LogDebug
		log.Println("Discord session debug logging enabled")
	}

	bot := &Bot{
		session: dg,
		store:   st,
		verify:  v,
		backup:  backup.NewScheduler(dg, st, cfg.Storage.BackupDir),
	}

	dg.AddHandler(bot.onReady)
	dg.AddHandler(bot.onInteractionCreate)

	dg.Identify.Intents = discordgo.IntentsGuilds |
		discordgo.IntentsGuildMembers |
		discordgo.IntentGuildModeration |
		discordgo.IntentsGuildEmojis |
		discordgo.IntentsGuildBans

	if err := dg.Open(); err != nil {
		log.Fatalf("Error connecting to Discord: %v", err)
	}
	defer dg.Close()

	// Start backup scheduler.
	bot.backup.Start()

	log.Println("Bot is running. Press CTRL-C to exit.")
	stop := make(chan os.Signal, 1)
	signal.Notify(stop, os.Interrupt, syscall.SIGTERM)
	<-stop
	log.Println("Shutting down...")
	bot.backup.Stop()
}

func (b *Bot) getLocale(i *discordgo.InteractionCreate) i18n.Locale {
	locale, _, err := b.store.GetUserLocale(context.Background(), i.GuildID, i.Member.User.ID)
	if err != nil || locale == "" {
		return i18n.LocaleEN
	}
	return i18n.ParseLocale(locale)
}

func (b *Bot) localizeError(i *discordgo.InteractionCreate, err error) string {
	locale := b.getLocale(i)
	t := i18n.Get(locale)

	var wce *verify.WrongCodeError
	if errors.As(err, &wce) {
		return fmt.Sprintf(t.ErrWrongCodeFmt, wce.Remaining)
	}

	switch {
	case errors.Is(err, verify.ErrNotActive):
		return t.ErrNotActive
	case errors.Is(err, verify.ErrRateLimited):
		return t.ErrRateLimited
	case errors.Is(err, verify.ErrNoPending):
		return t.ErrNoPending
	case errors.Is(err, verify.ErrExpired):
		return t.ErrExpired
	case errors.Is(err, verify.ErrTooManyAttempts):
		return t.ErrTooManyAttempts
	case errors.Is(err, verify.ErrSendFailed):
		return t.ErrSendFailed
	case errors.Is(err, verify.ErrEmailAlreadyUsed):
		return t.ErrEmailAlreadyUsed
	case errors.Is(err, verify.ErrInvalidDomain):
		return t.ErrInvalidDomain
	case errors.Is(err, verify.ErrMissingConfig):
		return t.ErrMissingConfig
	}

	return err.Error()
}

func (b *Bot) onReady(s *discordgo.Session, r *discordgo.Ready) {
	log.Printf("Logged in as %v#%v", s.State.User.Username, s.State.User.Discriminator)

	// Register commands globally
	en := i18n.Get(i18n.LocaleEN)
	commands := []*discordgo.ApplicationCommand{
		{
			Name:        "setup",
			Description: en.SetupDesc,
			Options: []*discordgo.ApplicationCommandOption{
				{
					Type:        discordgo.ApplicationCommandOptionString,
					Name:        "domain",
					Description: en.SetupDomain,
					Required:    true,
				},
				{
					Type:        discordgo.ApplicationCommandOptionString,
					Name:        "mode",
					Description: en.SetupMode,
					Required:    true,
					Choices: []*discordgo.ApplicationCommandOptionChoice{
						{Name: "Regex Matching", Value: "REGEX"},
						{Name: "CSV Mapping", Value: "CSV"},
					},
				},
				{
					Type:        discordgo.ApplicationCommandOptionChannel,
					Name:        "channel",
					Description: en.SetupChannel,
					Required:    true,
				},
				{
					Type:        discordgo.ApplicationCommandOptionString,
					Name:        "subject",
					Description: en.SetupSubject,
					Required:    false,
				},
			},
			DefaultMemberPermissions: func(i int64) *int64 { return &i }(discordgo.PermissionAdministrator),
		},
		{
			Name:        "regex",
			Description: en.RegexDesc,
			Options: []*discordgo.ApplicationCommandOption{
				{
					Type:        discordgo.ApplicationCommandOptionSubCommand,
					Name:        "add",
					Description: en.RegexAdd,
					Options: []*discordgo.ApplicationCommandOption{
						{Type: discordgo.ApplicationCommandOptionString, Name: "pattern", Description: en.RegexPattern, Required: true},
						{Type: discordgo.ApplicationCommandOptionRole, Name: "role", Description: en.RegexRole, Required: true},
						{Type: discordgo.ApplicationCommandOptionInteger, Name: "priority", Description: en.RegexPriority, Required: false},
					},
				},
				{
					Type:        discordgo.ApplicationCommandOptionSubCommand,
					Name:        "list",
					Description: en.RegexList,
				},
				{
					Type:        discordgo.ApplicationCommandOptionSubCommand,
					Name:        "remove",
					Description: en.RegexRemove,
					Options: []*discordgo.ApplicationCommandOption{
						{Type: discordgo.ApplicationCommandOptionInteger, Name: "id", Description: en.RegexID, Required: true},
					},
				},
				{
					Type:        discordgo.ApplicationCommandOptionSubCommand,
					Name:        "remove-all",
					Description: en.RegexRemoveAll,
				},
				{
					Type:        discordgo.ApplicationCommandOptionSubCommand,
					Name:        "remove-range",
					Description: en.RegexRemoveRange,
					Options: []*discordgo.ApplicationCommandOption{
						{Type: discordgo.ApplicationCommandOptionInteger, Name: "start_id", Description: "Start rule ID", Required: true},
						{Type: discordgo.ApplicationCommandOptionInteger, Name: "end_id", Description: "End rule ID", Required: true},
					},
				},
				{
					Type:        discordgo.ApplicationCommandOptionSubCommand,
					Name:        "import",
					Description: en.RegexImport,
					Options: []*discordgo.ApplicationCommandOption{
						{Type: discordgo.ApplicationCommandOptionAttachment, Name: "file", Description: en.RegexImportFile, Required: false},
						{Type: discordgo.ApplicationCommandOptionString, Name: "text", Description: en.RegexImportDesc, Required: false},
					},
				},
			},
			DefaultMemberPermissions: func(i int64) *int64 { return &i }(discordgo.PermissionAdministrator),
		},
		{
			Name:        "csv",
			Description: en.CsvDesc,
			Options: []*discordgo.ApplicationCommandOption{
				{
					Type:        discordgo.ApplicationCommandOptionSubCommand,
					Name:        "upload",
					Description: en.CsvUpload,
					Options: []*discordgo.ApplicationCommandOption{
						{Type: discordgo.ApplicationCommandOptionAttachment, Name: "file", Description: en.CsvFile, Required: true},
					},
				},
				{
					Type:        discordgo.ApplicationCommandOptionSubCommand,
					Name:        "map",
					Description: en.CsvMap,
					Options: []*discordgo.ApplicationCommandOption{
						{Type: discordgo.ApplicationCommandOptionString, Name: "class", Description: en.CsvClass, Required: true},
						{Type: discordgo.ApplicationCommandOptionRole, Name: "role", Description: en.CsvRole, Required: true},
					},
				},
			},
			DefaultMemberPermissions: func(i int64) *int64 { return &i }(discordgo.PermissionAdministrator),
		},
		{
			Name:        "ratelimit",
			Description: en.RateLimitDesc,
			Options: []*discordgo.ApplicationCommandOption{
				{
					Type:        discordgo.ApplicationCommandOptionInteger,
					Name:        "count",
					Description: en.RateLimitCountDesc,
					Required:    true,
				},
				{
					Type:        discordgo.ApplicationCommandOptionInteger,
					Name:        "window",
					Description: en.RateLimitWindowDesc,
					Required:    true,
				},
			},
			DefaultMemberPermissions: func(i int64) *int64 { return &i }(discordgo.PermissionAdministrator),
		},
		{
			Name:        "verifiedrole",
			Description: en.VerifiedRoleDesc,
			Options: []*discordgo.ApplicationCommandOption{
				{
					Type:        discordgo.ApplicationCommandOptionSubCommand,
					Name:        "set",
					Description: en.VerifiedRoleSet,
					Options: []*discordgo.ApplicationCommandOption{
						{Type: discordgo.ApplicationCommandOptionRole, Name: "role", Description: en.VerifiedRoleRole, Required: true},
					},
				},
				{
					Type:        discordgo.ApplicationCommandOptionSubCommand,
					Name:        "view",
					Description: en.VerifiedRoleView,
				},
				{
					Type:        discordgo.ApplicationCommandOptionSubCommand,
					Name:        "clear",
					Description: en.VerifiedRoleClear,
				},
			},
			DefaultMemberPermissions: func(i int64) *int64 { return &i }(discordgo.PermissionAdministrator),
		},
		{
			Name:        "language",
			Description: "Change bot language",
			Options: []*discordgo.ApplicationCommandOption{
				{
					Type:        discordgo.ApplicationCommandOptionString,
					Name:        "language",
					Description: en.LanguageDesc,
					Required:    true,
					Choices: []*discordgo.ApplicationCommandOptionChoice{
						{Name: "English", Value: "en"},
						{Name: "Čeština", Value: "cs"},
					},
				},
			},
		},
		{
			Name:        "help",
			Description: en.HelpDesc,
		},
		{
			Name:        "backup",
			Description: en.BackupDesc,
			Options: []*discordgo.ApplicationCommandOption{
				{
					Type:        discordgo.ApplicationCommandOptionSubCommand,
					Name:        "create",
					Description: en.BackupCreate,
					Options: []*discordgo.ApplicationCommandOption{
						{
							Type:        discordgo.ApplicationCommandOptionString,
							Name:        "scope",
							Description: en.BackupScope,
							Required:    false,
							Choices: []*discordgo.ApplicationCommandOptionChoice{
								{Name: "Single server", Value: "single"},
								{Name: "Multi-server", Value: "multi"},
							},
						},
						{
							Type:        discordgo.ApplicationCommandOptionString,
							Name:        "guild-id",
							Description: en.BackupGuildID,
							Required:    false,
						},
					},
				},
				{
					Type:        discordgo.ApplicationCommandOptionSubCommand,
					Name:        "restore",
					Description: en.BackupRestore,
					Options: []*discordgo.ApplicationCommandOption{
						{
							Type:        discordgo.ApplicationCommandOptionInteger,
							Name:        "id",
							Description: "Backup ID",
							Required:    true,
						},
						{
							Type:        discordgo.ApplicationCommandOptionString,
							Name:        "guild-id",
							Description: en.BackupGuildID,
							Required:    false,
						},
					},
				},
				{
					Type:        discordgo.ApplicationCommandOptionSubCommand,
					Name:        "list",
					Description: en.BackupList,
					Options: []*discordgo.ApplicationCommandOption{
						{
							Type:        discordgo.ApplicationCommandOptionString,
							Name:        "type",
							Description: en.BackupAllLabel,
							Required:    false,
							Choices: []*discordgo.ApplicationCommandOptionChoice{
								{Name: "All", Value: "all"},
								{Name: "Manual", Value: "manual"},
								{Name: "Scheduled", Value: "scheduled"},
							},
						},
					},
				},
				{
					Type:        discordgo.ApplicationCommandOptionSubCommand,
					Name:        "schedule",
					Description: en.BackupSchedule,
					Options: []*discordgo.ApplicationCommandOption{
						{
							Type:        discordgo.ApplicationCommandOptionString,
							Name:        "frequency",
							Description: en.BackupFreq,
							Required:    true,
							Choices: []*discordgo.ApplicationCommandOptionChoice{
								{Name: "Every 12 hours", Value: "12h"},
								{Name: "Daily", Value: "daily"},
								{Name: "Weekly", Value: "weekly"},
								{Name: "Every 2 weeks", Value: "biweekly"},
								{Name: "Monthly", Value: "monthly"},
								{Name: "Every 3 months", Value: "3months"},
								{Name: "Every 6 months", Value: "6months"},
							},
						},
						{
							Type:        discordgo.ApplicationCommandOptionString,
							Name:        "time",
							Description: en.BackupTimeOfDay,
							Required:    false,
						},
					},
				},
				{
					Type:        discordgo.ApplicationCommandOptionSubCommand,
					Name:        "schedule-off",
					Description: en.BackupScheduleOff,
				},
				{
					Type:        discordgo.ApplicationCommandOptionSubCommand,
					Name:        "delete",
					Description: en.BackupDelete,
					Options: []*discordgo.ApplicationCommandOption{
						{
							Type:        discordgo.ApplicationCommandOptionInteger,
							Name:        "id",
							Description: "Backup ID",
							Required:    true,
						},
					},
				},
			},
			DefaultMemberPermissions: func(i int64) *int64 { return &i }(discordgo.PermissionAdministrator),
		},
	}

	_, err := s.ApplicationCommandBulkOverwrite(s.State.User.ID, "", commands)
	if err != nil {
		log.Printf("Error registering commands: %v", err)
	}
}

func (b *Bot) onInteractionCreate(s *discordgo.Session, i *discordgo.InteractionCreate) {
	if *debugMode {
		cmdName := ""
		if i.Type == discordgo.InteractionApplicationCommand {
			cmdName = i.ApplicationCommandData().Name
		} else if i.Type == discordgo.InteractionModalSubmit {
			cmdName = i.ModalSubmitData().CustomID
		} else {
			cmdName = fmt.Sprintf("component:%s", i.MessageComponentData().CustomID)
		}
		log.Printf("[DEBUG] Interaction: type=%d guild=%s user=%s cmd=%s", i.Type, i.GuildID, i.Member.User.ID, cmdName)
	}
	switch i.Type {
	case discordgo.InteractionApplicationCommand:
		b.handleSlashCommand(s, i)
	case discordgo.InteractionMessageComponent:
		b.handleComponent(s, i)
	case discordgo.InteractionModalSubmit:
		b.handleModal(s, i)
	}
}

func (b *Bot) handleSlashCommand(s *discordgo.Session, i *discordgo.InteractionCreate) {
	name := i.ApplicationCommandData().Name
	switch name {
	case "setup":
		b.cmdSetup(s, i)
	case "regex":
		b.cmdRegex(s, i)
	case "csv":
		b.cmdCSV(s, i)
	case "language":
		b.cmdLanguage(s, i)
	case "ratelimit":
		b.cmdRateLimit(s, i)
	case "verifiedrole":
		b.cmdVerifiedRole(s, i)
	case "backup":
		b.cmdBackup(s, i)
	case "help":
		b.cmdHelp(s, i)
	}
}

func (b *Bot) cmdSetup(s *discordgo.Session, i *discordgo.InteractionCreate) {
	t := i18n.Get(b.getLocale(i))
	opts := i.ApplicationCommandData().Options
	domain, domainOK := getStringOption(opts, "domain")
	mode, modeOK := getStringOption(opts, "mode")
	channelID, channelOK := getChannelOption(opts, "channel")
	subject, _ := getStringOption(opts, "subject")
	if !domainOK || !modeOK || !channelOK {
		respondErr(s, i, "Missing required argument.")
		return
	}
	if subject == "" {
		subject = t.DefaultSubject
	}

	cfg := store.GuildConfig{
		GuildID:         i.GuildID,
		VerifyChannelID: channelID,
		Domain:          domain,
		Mode:            mode,
		Subject:         subject,
		CodeTTL:         10 * time.Minute,
		MaxAttempts:     5,
		RateLimitCount:  3,
		RateLimitWindow: 15 * time.Minute,
	}

	if existing, ok, err := b.store.GetGuildConfig(context.Background(), i.GuildID); err == nil && ok {
		cfg.DefaultRoleID = existing.DefaultRoleID
	}

	if err := b.store.SaveGuildConfig(context.Background(), cfg); err != nil {
		respondErr(s, i, t.FailedSave)
		return
	}

	_, err := s.ChannelMessageSendComplex(channelID, &discordgo.MessageSend{
		Embeds: []*discordgo.MessageEmbed{{
			Title:       t.EmbedTitle,
			Description: fmt.Sprintf(t.EmbedDescFmt, domain),
			Color:       0x3b82f6,
		}},
		Components: []discordgo.MessageComponent{
			discordgo.ActionsRow{
				Components: []discordgo.MessageComponent{
					discordgo.Button{
						CustomID: "btn_verify_start",
						Label:    t.VerifyBtn,
						Style:    discordgo.PrimaryButton,
					},
				},
			},
		},
	})

	if err != nil {
		respondErr(s, i, t.ConfigSavedErr)
		return
	}

	respondOK(s, i, t.ConfigSaved)
}

func (b *Bot) cmdRegex(s *discordgo.Session, i *discordgo.InteractionCreate) {
	t := i18n.Get(b.getLocale(i))
	subcmd, ok := getSubCommandOption(i.ApplicationCommandData().Options)
	if !ok {
		respondErr(s, i, "Missing subcommand.")
		return
	}
	switch subcmd.Name {
	case "add":
		pattern, patternOK := getStringOption(subcmd.Options, "pattern")
		role, roleOK := getRoleOption(subcmd.Options, "role")
		priority, _ := getIntOption(subcmd.Options, "priority")
		if !patternOK || !roleOK {
			respondErr(s, i, "Missing required argument.")
			return
		}
		err := b.store.AddRegexRule(context.Background(), store.RegexRule{
			GuildID:  i.GuildID,
			Pattern:  pattern,
			RoleID:   role.ID,
			Priority: int(priority),
		})
		if err != nil {
			respondErr(s, i, t.FailedSave)
			return
		}
		respondOK(s, i, t.RuleAdded)

	case "list":
		rules, err := b.store.ListRegexRules(context.Background(), i.GuildID)
		if err != nil {
			respondErr(s, i, t.FailedLoadRules)
			return
		}
		if len(rules) == 0 {
			respondOK(s, i, t.NoRules)
			return
		}
		var msg strings.Builder
		for _, r := range rules {
			msg.WriteString(fmt.Sprintf("ID: %d | Pattern: `%s` | Role: <@&%s> | Priority: %d\n", r.ID, r.Pattern, r.RoleID, r.Priority))
		}
		respondOK(s, i, msg.String())

	case "remove":
		idValue, ok := getIntOption(subcmd.Options, "id")
		if !ok {
			respondErr(s, i, "Missing required argument.")
			return
		}
		if err := b.store.RemoveRegexRule(context.Background(), int(idValue)); err != nil {
			respondErr(s, i, t.FailedDelete)
			return
		}
		respondOK(s, i, t.RuleDeleted)

	case "remove-all":
		rules, err := b.store.ListRegexRules(context.Background(), i.GuildID)
		if err != nil {
			respondErr(s, i, t.FailedLoadRules)
			return
		}
		if len(rules) == 0 {
			respondOK(s, i, t.NoRules)
			return
		}

		components := []discordgo.MessageComponent{
			discordgo.ActionsRow{
				Components: []discordgo.MessageComponent{
					discordgo.Button{
						CustomID: fmt.Sprintf("regex_confirm_remove_all:%s", i.GuildID),
						Label:    "Confirm Delete All",
						Style:    discordgo.DangerButton,
					},
					discordgo.Button{
						CustomID: "regex_cancel",
						Label:    "Cancel",
						Style:    discordgo.SecondaryButton,
					},
				},
			},
		}

		err = s.InteractionRespond(i.Interaction, &discordgo.InteractionResponse{
			Type: discordgo.InteractionResponseChannelMessageWithSource,
			Data: &discordgo.InteractionResponseData{
				Content:    t.RegexConfirmAll,
				Components: components,
				Flags:      discordgo.MessageFlagsEphemeral,
			},
		})
		if err != nil {
			respondErr(s, i, t.FailedSave)
		}

	case "remove-range":
		startValue, startOK := getIntOption(subcmd.Options, "start_id")
		endValue, endOK := getIntOption(subcmd.Options, "end_id")
		if !startOK || !endOK {
			respondErr(s, i, "Missing required argument.")
			return
		}
		startID := int(startValue)
		endID := int(endValue)
		if startID > endID {
			startID, endID = endID, startID
		}

		rules, err := b.store.ListRegexRules(context.Background(), i.GuildID)
		if err != nil {
			respondErr(s, i, t.FailedLoadRules)
			return
		}

		// Check if any rules exist in range
		hasRules := false
		for _, r := range rules {
			if r.ID >= startID && r.ID <= endID {
				hasRules = true
				break
			}
		}

		if !hasRules {
			respondOK(s, i, "No rules found in the specified range.")
			return
		}

		components := []discordgo.MessageComponent{
			discordgo.ActionsRow{
				Components: []discordgo.MessageComponent{
					discordgo.Button{
						CustomID: fmt.Sprintf("regex_confirm_remove_range:%s:%d:%d", i.GuildID, startID, endID),
						Label:    "Confirm Delete Range",
						Style:    discordgo.DangerButton,
					},
					discordgo.Button{
						CustomID: "regex_cancel",
						Label:    "Cancel",
						Style:    discordgo.SecondaryButton,
					},
				},
			},
		}

		err = s.InteractionRespond(i.Interaction, &discordgo.InteractionResponse{
			Type: discordgo.InteractionResponseChannelMessageWithSource,
			Data: &discordgo.InteractionResponseData{
				Content:    fmt.Sprintf(t.RegexConfirmRange+"\nRange: %d - %d", startID, endID),
				Components: components,
				Flags:      discordgo.MessageFlagsEphemeral,
			},
		})
		if err != nil {
			respondErr(s, i, t.FailedSave)
		}

	case "import":
		text, textOK := getStringOption(subcmd.Options, "text")
		attachmentID, fileOK := getAttachmentOption(subcmd.Options, "file")
		if !fileOK && !textOK {
			respondErr(s, i, "Please provide either a file or text input.")
			return
		}

		if fileOK {
			att, ok := i.ApplicationCommandData().Resolved.Attachments[attachmentID]
			if !ok || att == nil || att.URL == "" {
				respondErr(s, i, t.ErrorDownload)
				return
			}
			resp, err := http.Get(att.URL)
			if err != nil || resp.StatusCode != http.StatusOK {
				respondErr(s, i, t.ErrorDownload)
				return
			}
			defer resp.Body.Close()

			buf := new(strings.Builder)
			_, err = io.Copy(buf, resp.Body)
			if err != nil {
				respondErr(s, i, "Failed to read file.")
				return
			}
			text = buf.String()
		}

		lines := strings.Split(text, "\n")
		var rulesToImport []store.RegexRule
		priority := len(lines) // Start with high priority

		for _, line := range lines {
			line = strings.TrimSpace(line)
			if line == "" || strings.HasPrefix(line, "#") {
				continue
			}

			// Parse CSV format: regex;role_id
			parts := strings.Split(line, ";")
			if len(parts) != 2 {
				continue // Skip invalid lines
			}

			pattern := strings.TrimSpace(parts[0])
			roleID := strings.TrimSpace(parts[1])

			if pattern == "" || roleID == "" {
				continue
			}

			rulesToImport = append(rulesToImport, store.RegexRule{
				GuildID:  i.GuildID,
				Pattern:  pattern,
				RoleID:   roleID,
				Priority: priority,
			})
			priority--
		}

		if len(rulesToImport) == 0 {
			respondOK(s, i, "No valid rules found in input.")
			return
		}

		err := b.store.BulkInsertRegexRules(context.Background(), i.GuildID, rulesToImport)
		if err != nil {
			respondErr(s, i, t.FailedSave)
			return
		}

		respondOK(s, i, fmt.Sprintf("Imported %d regex rules.", len(rulesToImport)))
	}
}

func (b *Bot) cmdCSV(s *discordgo.Session, i *discordgo.InteractionCreate) {
	t := i18n.Get(b.getLocale(i))
	subcmd, ok := getSubCommandOption(i.ApplicationCommandData().Options)
	if !ok {
		respondErr(s, i, "Missing subcommand.")
		return
	}
	switch subcmd.Name {
	case "upload":
		attachmentID, ok := getAttachmentOption(subcmd.Options, "file")
		if !ok {
			respondErr(s, i, "Missing required argument.")
			return
		}
		att, ok := i.ApplicationCommandData().Resolved.Attachments[attachmentID]
		if !ok || att == nil || att.URL == "" {
			respondErr(s, i, t.ErrorDownload)
			return
		}

		resp, err := http.Get(att.URL)
		if err != nil || resp.StatusCode != http.StatusOK {
			respondErr(s, i, t.ErrorDownload)
			return
		}
		defer resp.Body.Close()

		reader := csv.NewReader(resp.Body)
		records, err := reader.ReadAll()
		if err != nil {
			respondErr(s, i, t.InvalidCSV)
			return
		}

		ctx := context.Background()
		_ = b.store.ClearCSVEmails(ctx, i.GuildID)

		count := 0
		for _, row := range records {
			if len(row) >= 2 {
				email, class := strings.TrimSpace(row[0]), strings.TrimSpace(row[1])
				if email != "" && class != "" {
					b.store.InsertCSVEmail(ctx, i.GuildID, email, class)
					count++
				}
			}
		}
		respondOK(s, i, fmt.Sprintf(t.UploadedEmailsFmt, count))

	case "map":
		class, classOK := getStringOption(subcmd.Options, "class")
		role, roleOK := getRoleOption(subcmd.Options, "role")
		if !classOK || !roleOK {
			respondErr(s, i, "Missing required argument.")
			return
		}
		err := b.store.MapCSVClass(context.Background(), i.GuildID, class, role.ID)
		if err != nil {
			respondErr(s, i, t.FailedMap)
			return
		}
		respondOK(s, i, fmt.Sprintf(t.ClassMappedFmt, class, role.ID))
	}
}

func (b *Bot) handleComponent(s *discordgo.Session, i *discordgo.InteractionCreate) {
	t := i18n.Get(b.getLocale(i))
	customID := i.MessageComponentData().CustomID

	switch {
	case customID == "btn_verify_start":
		err := s.InteractionRespond(i.Interaction, &discordgo.InteractionResponse{
			Type: discordgo.InteractionResponseModal,
			Data: &discordgo.InteractionResponseData{
				CustomID: "modal_email",
				Title:    t.VerifyModalTitle,
				Components: []discordgo.MessageComponent{
					discordgo.ActionsRow{
						Components: []discordgo.MessageComponent{
							discordgo.TextInput{
								CustomID:    "input_email",
								Label:       t.YourEmail,
								Style:       discordgo.TextInputShort,
								Placeholder: t.EmailPlaceholder,
								Required:    ptrBool(true),
							},
						},
					},
				},
			},
		})
		if err != nil {
			log.Println("Error sending modal:", err)
		}
	case customID == "btn_enter_code":
		err := s.InteractionRespond(i.Interaction, &discordgo.InteractionResponse{
			Type: discordgo.InteractionResponseModal,
			Data: &discordgo.InteractionResponseData{
				CustomID: "modal_code",
				Title:    t.CodeModalTitle,
				Components: []discordgo.MessageComponent{
					discordgo.ActionsRow{
						Components: []discordgo.MessageComponent{
							discordgo.TextInput{
								CustomID:    "input_code",
								Label:       t.CodeLabel,
								Style:       discordgo.TextInputShort,
								Placeholder: t.CodePlaceholder,
								Required:    ptrBool(true),
								MinLength:   6,
								MaxLength:   6,
							},
						},
					},
				},
			},
		})
		if err != nil {
			log.Println("Error sending modal:", err)
		}
	case strings.HasPrefix(customID, "regex_confirm_remove_all:"):
		guildID := strings.TrimPrefix(customID, "regex_confirm_remove_all:")
		if guildID != i.GuildID {
			respondErr(s, i, "Invalid guild.")
			return
		}

		err := b.store.RemoveAllRegexRules(context.Background(), guildID)
		if err != nil {
			respondErr(s, i, t.FailedDelete)
			return
		}

		err = s.InteractionRespond(i.Interaction, &discordgo.InteractionResponse{
			Type: discordgo.InteractionResponseUpdateMessage,
			Data: &discordgo.InteractionResponseData{
				Content:    "All regex rules have been deleted.",
				Components: []discordgo.MessageComponent{},
			},
		})
		if err != nil {
			log.Println("Error updating message:", err)
		}

	case strings.HasPrefix(customID, "regex_confirm_remove_range:"):
		parts := strings.Split(strings.TrimPrefix(customID, "regex_confirm_remove_range:"), ":")
		if len(parts) != 3 {
			respondErr(s, i, "Invalid range data.")
			return
		}

		guildID := parts[0]
		if guildID != i.GuildID {
			respondErr(s, i, "Invalid guild.")
			return
		}

		startID := 0
		endID := 0
		fmt.Sscanf(parts[1], "%d", &startID)
		fmt.Sscanf(parts[2], "%d", &endID)

		err := b.store.RemoveRegexRulesRange(context.Background(), guildID, startID, endID)
		if err != nil {
			respondErr(s, i, t.FailedDelete)
			return
		}

		err = s.InteractionRespond(i.Interaction, &discordgo.InteractionResponse{
			Type: discordgo.InteractionResponseUpdateMessage,
			Data: &discordgo.InteractionResponseData{
				Content:    fmt.Sprintf("Rules in range %d - %d have been deleted.", startID, endID),
				Components: []discordgo.MessageComponent{},
			},
		})
		if err != nil {
			log.Println("Error updating message:", err)
		}

	case customID == "regex_cancel":
		err := s.InteractionRespond(i.Interaction, &discordgo.InteractionResponse{
			Type: discordgo.InteractionResponseUpdateMessage,
			Data: &discordgo.InteractionResponseData{
				Content:    "Operation cancelled.",
				Components: []discordgo.MessageComponent{},
			},
		})
		if err != nil {
			log.Println("Error updating message:", err)
		}
	}
}

func (b *Bot) handleModal(s *discordgo.Session, i *discordgo.InteractionCreate) {
	t := i18n.Get(b.getLocale(i))
	data := i.ModalSubmitData()

	switch data.CustomID {
	case "modal_email":
		_ = s.InteractionRespond(i.Interaction, &discordgo.InteractionResponse{
			Type: discordgo.InteractionResponseDeferredChannelMessageWithSource,
			Data: &discordgo.InteractionResponseData{
				Flags: discordgo.MessageFlagsEphemeral,
			},
		})

		go func() {
			email := data.Components[0].(*discordgo.ActionsRow).Components[0].(*discordgo.TextInput).Value
			userLocale := b.getLocale(i)
			if l, _, err := b.store.GetUserLocale(context.Background(), i.GuildID, i.Member.User.ID); err == nil && l != "" {
				userLocale = i18n.ParseLocale(l)
			}
			err := b.verify.Start(context.Background(), i.GuildID, i.Member.User.ID, email, userLocale)
			if err != nil {
				_, _ = s.FollowupMessageCreate(i.Interaction, true, &discordgo.WebhookParams{
					Content: "❌ " + fmt.Sprintf(t.ErrEmailFmt, b.localizeError(i, err)),
					Flags:   discordgo.MessageFlagsEphemeral,
				})
				return
			}

			_, _ = s.FollowupMessageCreate(i.Interaction, true, &discordgo.WebhookParams{
				Content: fmt.Sprintf(t.CodeSentFmt, email),
				Flags:   discordgo.MessageFlagsEphemeral,
				Components: []discordgo.MessageComponent{
					discordgo.ActionsRow{
						Components: []discordgo.MessageComponent{
							discordgo.Button{
								CustomID: "btn_enter_code",
								Label:    t.EnterCodeBtn,
								Style:    discordgo.SuccessButton,
							},
						},
					},
				},
			})
		}()

	case "modal_code":
		_ = s.InteractionRespond(i.Interaction, &discordgo.InteractionResponse{
			Type: discordgo.InteractionResponseDeferredChannelMessageWithSource,
			Data: &discordgo.InteractionResponseData{
				Flags: discordgo.MessageFlagsEphemeral,
			},
		})

		go func() {
			code := data.Components[0].(*discordgo.ActionsRow).Components[0].(*discordgo.TextInput).Value
			userLocale := b.getLocale(i)
			if l, _, err := b.store.GetUserLocale(context.Background(), i.GuildID, i.Member.User.ID); err == nil && l != "" {
				userLocale = i18n.ParseLocale(l)
			}
			roleIDs, err := b.verify.Confirm(context.Background(), i.GuildID, i.Member.User.ID, code)
			if err != nil {
				_, _ = s.FollowupMessageCreate(i.Interaction, true, &discordgo.WebhookParams{
					Content: "❌ " + b.localizeError(i, err),
					Flags:   discordgo.MessageFlagsEphemeral,
				})
				return
			}

			for _, roleID := range roleIDs {
				if err := s.GuildMemberRoleAdd(i.GuildID, i.Member.User.ID, roleID); err != nil {
					_, _ = s.FollowupMessageCreate(i.Interaction, true, &discordgo.WebhookParams{
						Content: "❌ " + i18n.Get(userLocale).ErrSendFailed,
						Flags:   discordgo.MessageFlagsEphemeral,
					})
					return
				}
			}

			_, _ = s.FollowupMessageCreate(i.Interaction, true, &discordgo.WebhookParams{
				Content: "✅ " + i18n.Get(userLocale).VerifySuccess,
				Flags:   discordgo.MessageFlagsEphemeral,
			})
		}()
	}
}

func (b *Bot) cmdLanguage(s *discordgo.Session, i *discordgo.InteractionCreate) {
	t := i18n.Get(b.getLocale(i))
	localeStr, ok := getStringOption(i.ApplicationCommandData().Options, "language")
	if !ok {
		respondErr(s, i, "Missing required argument.")
		return
	}
	locale := i18n.ParseLocale(localeStr)
	if err := b.store.SetUserLocale(context.Background(), i.GuildID, i.Member.User.ID, string(locale)); err != nil {
		respondErr(s, i, t.FailedSave)
		return
	}
	langName := localeStr
	if locale == i18n.LocaleEN {
		langName = "English"
	} else if locale == i18n.LocaleCS {
		langName = "Čeština"
	}
	respondOK(s, i, fmt.Sprintf(t.LanguageSetFmt, langName))
}

func (b *Bot) cmdRateLimit(s *discordgo.Session, i *discordgo.InteractionCreate) {
	t := i18n.Get(b.getLocale(i))
	count, countOK := getIntOption(i.ApplicationCommandData().Options, "count")
	window, windowOK := getIntOption(i.ApplicationCommandData().Options, "window")
	if !countOK || !windowOK {
		respondErr(s, i, "Missing required argument.")
		return
	}

	if count < 1 || count > 3 || window < 15 || window > 60 {
		respondErr(s, i, t.FailedSave)
		return
	}

	cfg, ok, err := b.store.GetGuildConfig(context.Background(), i.GuildID)
	if err != nil {
		respondErr(s, i, t.FailedSave)
		return
	}
	if !ok {
		respondErr(s, i, i18n.Get(i18n.LocaleEN).ErrMissingConfig)
		return
	}

	cfg.RateLimitCount = int(count)
	cfg.RateLimitWindow = time.Duration(window) * time.Minute

	if err := b.store.SaveGuildConfig(context.Background(), cfg); err != nil {
		respondErr(s, i, t.FailedSave)
		return
	}

	respondOK(s, i, fmt.Sprintf("%s %d / %d min.", t.RateLimitSetFmt, cfg.RateLimitCount, int(cfg.RateLimitWindow.Minutes())))
}

func (b *Bot) cmdVerifiedRole(s *discordgo.Session, i *discordgo.InteractionCreate) {
	subcmd, ok := getSubCommandOption(i.ApplicationCommandData().Options)
	if !ok {
		respondErr(s, i, "Missing subcommand.")
		return
	}

	ctx := context.Background()
	switch subcmd.Name {
	case "set":
		role, ok := getRoleOption(subcmd.Options, "role")
		if !ok {
			respondErr(s, i, "Missing required argument.")
			return
		}
		t := i18n.Get(b.getLocale(i))
		cfg, ok, err := b.store.GetGuildConfig(ctx, i.GuildID)
		if err != nil {
			respondErr(s, i, t.FailedSave)
			return
		}
		if !ok {
			respondErr(s, i, i18n.Get(i18n.LocaleEN).ErrMissingConfig)
			return
		}
		cfg.DefaultRoleID = role.ID
		if err := b.store.SaveGuildConfig(ctx, cfg); err != nil {
			respondErr(s, i, t.FailedSave)
			return
		}
		respondOK(s, i, fmt.Sprintf(t.VerifiedRoleSetFmt, cfg.DefaultRoleID))

	case "view", "clear":
		t := i18n.Get(b.getLocale(i))
		cfg, ok, err := b.store.GetGuildConfig(ctx, i.GuildID)
		if err != nil {
			respondErr(s, i, t.FailedSave)
			return
		}
		if !ok {
			respondErr(s, i, i18n.Get(i18n.LocaleEN).ErrMissingConfig)
			return
		}
		if subcmd.Name == "view" {
			if cfg.DefaultRoleID == "" {
				respondOK(s, i, t.VerifiedRoleNotSet)
				return
			}
			respondOK(s, i, fmt.Sprintf(t.VerifiedRoleViewFmt, cfg.DefaultRoleID))
			return
		}
		cfg.DefaultRoleID = ""
		if err := b.store.SaveGuildConfig(ctx, cfg); err != nil {
			respondErr(s, i, t.FailedSave)
			return
		}
		respondOK(s, i, t.VerifiedRoleCleared)
	}
}

func (b *Bot) cmdHelp(s *discordgo.Session, i *discordgo.InteractionCreate) {
	t := i18n.Get(b.getLocale(i))

	description := t.HelpClickHint + "\n\n"
	description += "**" + t.HelpAdminTitle + "**\n"
	description += "`/setup` - " + t.SetupDesc + "\n"
	description += "`/regex` - " + t.RegexDesc + "\n"
	description += "`/csv` - " + t.CsvDesc + "\n"
	description += "`/ratelimit` - " + t.RateLimitDesc + "\n"
	description += "`/verifiedrole` - " + t.VerifiedRoleDesc + "\n\n"
	description += "**" + t.HelpUserTitle + "**\n"
	description += "`/language` - " + t.LanguageDesc + "\n"
	description += "`/help` - " + t.HelpDesc

	_ = s.InteractionRespond(i.Interaction, &discordgo.InteractionResponse{
		Type: discordgo.InteractionResponseChannelMessageWithSource,
		Data: &discordgo.InteractionResponseData{
			Embeds: []*discordgo.MessageEmbed{{
				Title:       t.HelpText,
				Description: description,
				Color:       0x3b82f6,
			}},
			Flags: discordgo.MessageFlagsEphemeral,
		},
	})
}

func respondOK(s *discordgo.Session, i *discordgo.InteractionCreate, msg string) {
	_ = s.InteractionRespond(i.Interaction, &discordgo.InteractionResponse{
		Type: discordgo.InteractionResponseChannelMessageWithSource,
		Data: &discordgo.InteractionResponseData{
			Content: "✅ " + msg,
			Flags:   discordgo.MessageFlagsEphemeral,
		},
	})
}

func respondErr(s *discordgo.Session, i *discordgo.InteractionCreate, msg string) {
	_ = s.InteractionRespond(i.Interaction, &discordgo.InteractionResponse{
		Type: discordgo.InteractionResponseChannelMessageWithSource,
		Data: &discordgo.InteractionResponseData{
			Content: "❌ " + msg,
			Flags:   discordgo.MessageFlagsEphemeral,
		},
	})
}

// --- Backup commands ---

func (b *Bot) cmdBackup(s *discordgo.Session, i *discordgo.InteractionCreate) {
	t := i18n.Get(b.getLocale(i))
	subcmd, ok := getSubCommandOption(i.ApplicationCommandData().Options)
	if !ok {
		respondErr(s, i, "Missing subcommand.")
		return
	}
	switch subcmd.Name {
	case "create":
		b.backupCreate(s, i, subcmd, t)
	case "restore":
		b.backupRestore(s, i, subcmd, t)
	case "list":
		b.backupList(s, i, subcmd, t)
	case "schedule":
		b.backupSchedule(s, i, subcmd, t)
	case "schedule-off":
		b.backupScheduleOff(s, i, t)
	case "delete":
		b.backupDelete(s, i, subcmd, t)
	}
}

func (b *Bot) backupCreate(s *discordgo.Session, i *discordgo.InteractionCreate, subcmd *discordgo.ApplicationCommandInteractionDataOption, t i18n.Translations) {
	scope := backup.ScopeSingle
	if scopeValue, ok := getStringOption(subcmd.Options, "scope"); ok && scopeValue == "multi" {
		scope = backup.ScopeMulti
	}
	guildID := i.GuildID
	if guildIDValue, ok := getStringOption(subcmd.Options, "guild-id"); ok && guildIDValue != "" {
		guildID = guildIDValue
	}

	data, err := backup.CaptureGuild(s, guildID)
	if err != nil {
		respondErr(s, i, t.BackupErrorCapture)
		return
	}
	data.Scope = scope

	assetsDir := filepath.Join(b.backup.BackupDir(), "assets", fmt.Sprintf("backup_%d", time.Now().UnixNano()))
	if err := backup.DownloadEmojiAssets(data, assetsDir); err != nil {
		log.Printf("[backup] emoji download: %v", err)
	}

	if err := os.MkdirAll(b.backup.BackupDir(), 0o755); err != nil {
		respondErr(s, i, t.BackupErrorSave)
		return
	}
	filePath := filepath.Join(b.backup.BackupDir(), fmt.Sprintf("backup_%s_%s.json", guildID, time.Now().Format("20060102_150405")))
	if err := backup.WriteJSON(filePath, data); err != nil {
		respondErr(s, i, t.BackupErrorSave)
		return
	}

	rec := store.BackupRecord{
		GuildID:      guildID,
		Scope:        string(scope),
		Kind:         string(backup.KindManual),
		Filepath:     filePath,
		CreatedAt:    time.Now().UTC(),
		ChannelCount: len(data.Channels),
		RoleCount:    len(data.Roles),
		EmojiCount:   len(data.Emojis),
		BanCount:     len(data.Bans),
	}
	if _, err := b.store.SaveBackup(context.Background(), rec); err != nil {
		respondErr(s, i, t.BackupErrorSave)
		return
	}

	respondOK(s, i, fmt.Sprintf(t.BackupCreatedFmt, filePath))
}

func (b *Bot) backupRestore(s *discordgo.Session, i *discordgo.InteractionCreate, subcmd *discordgo.ApplicationCommandInteractionDataOption, t i18n.Translations) {
	id, ok := getIntOption(subcmd.Options, "id")
	if !ok {
		respondErr(s, i, "Missing required argument.")
		return
	}
	targetGuildID := i.GuildID
	if guildID, ok := getStringOption(subcmd.Options, "guild-id"); ok && guildID != "" {
		targetGuildID = guildID
	}

	rec, ok, err := b.store.GetBackup(context.Background(), int(id))
	if err != nil || !ok {
		respondErr(s, i, t.BackupErrorList)
		return
	}

	var data backup.BackupData
	if err := backup.ReadJSON(rec.Filepath, &data); err != nil {
		respondErr(s, i, t.BackupErrorRestore)
		return
	}

	if err := backup.RestoreGuild(s, &data, targetGuildID); err != nil {
		respondErr(s, i, t.BackupErrorRestore)
		return
	}

	respondOK(s, i, fmt.Sprintf(t.BackupRestoredFmt, rec.Filepath))
}

func (b *Bot) backupList(s *discordgo.Session, i *discordgo.InteractionCreate, subcmd *discordgo.ApplicationCommandInteractionDataOption, t i18n.Translations) {
	kind := ""
	if kindValue, ok := getStringOption(subcmd.Options, "type"); ok && kindValue != "all" {
		kind = kindValue
	}
	records, err := b.store.ListBackups(context.Background(), i.GuildID, kind)
	if err != nil {
		respondErr(s, i, t.BackupErrorList)
		return
	}
	if len(records) == 0 {
		respondOK(s, i, t.BackupNoBackups)
		return
	}
	var msg strings.Builder
	for _, r := range records {
		msg.WriteString(fmt.Sprintf("ID: %d | %s | %s | %s | %d channels, %d roles, %d emojis, %d bans\n",
			r.ID, r.Kind, r.Scope, r.CreatedAt.Format("2006-01-02 15:04"), r.ChannelCount, r.RoleCount, r.EmojiCount, r.BanCount))
	}
	respondOK(s, i, msg.String())
}

func (b *Bot) backupSchedule(s *discordgo.Session, i *discordgo.InteractionCreate, subcmd *discordgo.ApplicationCommandInteractionDataOption, t i18n.Translations) {
	frequency, ok := getStringOption(subcmd.Options, "frequency")
	if !ok {
		respondErr(s, i, "Missing required argument.")
		return
	}
	timeOfDay := "00:00"
	if value, ok := getStringOption(subcmd.Options, "time"); ok && value != "" {
		timeOfDay = value
	}

	cfg := store.ScheduledBackupConfig{
		GuildID:   i.GuildID,
		Enabled:   true,
		Frequency: frequency,
		TimeOfDay: timeOfDay,
		SlotCount: 3,
		NextRun:   time.Now().Add(backup.FrequencyInterval(frequency)),
	}
	if err := b.store.SaveScheduledConfig(context.Background(), cfg); err != nil {
		respondErr(s, i, t.BackupErrorSave)
		return
	}

	respondOK(s, i, fmt.Sprintf(t.BackupScheduledFmt, frequency))
}

func (b *Bot) backupScheduleOff(s *discordgo.Session, i *discordgo.InteractionCreate, t i18n.Translations) {
	cfg := store.ScheduledBackupConfig{
		GuildID:   i.GuildID,
		Enabled:   false,
		SlotCount: 3,
	}
	if err := b.store.SaveScheduledConfig(context.Background(), cfg); err != nil {
		respondErr(s, i, t.BackupErrorSave)
		return
	}
	respondOK(s, i, t.BackupScheduledOff)
}

func (b *Bot) backupDelete(s *discordgo.Session, i *discordgo.InteractionCreate, subcmd *discordgo.ApplicationCommandInteractionDataOption, t i18n.Translations) {
	idValue, ok := getIntOption(subcmd.Options, "id")
	if !ok {
		respondErr(s, i, "Missing required argument.")
		return
	}
	id := int(idValue)
	rec, ok, err := b.store.GetBackup(context.Background(), id)
	if err != nil || !ok {
		respondErr(s, i, t.BackupErrorDelete)
		return
	}
	_ = os.Remove(rec.Filepath)
	if err := b.store.DeleteBackup(context.Background(), id); err != nil {
		respondErr(s, i, t.BackupErrorDelete)
		return
	}
	respondOK(s, i, fmt.Sprintf(t.BackupDeletedFmt, rec.Filepath))
}
