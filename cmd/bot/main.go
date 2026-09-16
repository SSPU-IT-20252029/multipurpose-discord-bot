package main

import (
	"context"
	"encoding/csv"
	"errors"
	"flag"
	"fmt"
	"log"
	"net/http"
	"os"
	"os/signal"
	"strings"
	"syscall"
	"time"

	"github.com/bwmarrin/discordgo"

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

type Bot struct {
	session *discordgo.Session
	store   *store.Store
	verify  *verify.Service
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
	}

	dg.AddHandler(bot.onReady)
	dg.AddHandler(bot.onInteractionCreate)

	dg.Identify.Intents = discordgo.IntentsGuilds

	if err := dg.Open(); err != nil {
		log.Fatalf("Error connecting to Discord: %v", err)
	}
	defer dg.Close()

	log.Println("Bot is running. Press CTRL-C to exit.")
	stop := make(chan os.Signal, 1)
	signal.Notify(stop, os.Interrupt, syscall.SIGTERM)
	<-stop
	log.Println("Shutting down...")
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
	case "help":
		b.cmdHelp(s, i)
	}
}

func (b *Bot) cmdSetup(s *discordgo.Session, i *discordgo.InteractionCreate) {
	t := i18n.Get(b.getLocale(i))
	opts := i.ApplicationCommandData().Options
	var domain, mode, channelID, subject string
	subject = t.DefaultSubject
	for _, o := range opts {
		switch o.Name {
		case "domain":
			domain = o.StringValue()
		case "mode":
			mode = o.StringValue()
		case "channel":
			channelID = o.ChannelValue(nil).ID
		case "subject":
			subject = o.StringValue()
		}
	}

	cfg := store.GuildConfig{
		GuildID:          i.GuildID,
		VerifyChannelID:  channelID,
		Domain:           domain,
		Mode:             mode,
		Subject:          subject,
		CodeTTL:          10 * time.Minute,
		MaxAttempts:      5,
		RateLimitCount:   3,
		RateLimitWindow:  15 * time.Minute,
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
	subcmd := i.ApplicationCommandData().Options[0]
	switch subcmd.Name {
	case "add":
		var pattern, roleID string
		priority := 0
		for _, o := range subcmd.Options {
			switch o.Name {
			case "pattern":
				pattern = o.StringValue()
			case "role":
				roleID = o.RoleValue(nil, "").ID
			case "priority":
				priority = int(o.IntValue())
			}
		}
		err := b.store.AddRegexRule(context.Background(), store.RegexRule{
			GuildID:    i.GuildID,
			Pattern:    pattern,
			RoleID:     roleID,
			Priority:   priority,
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
		id := int(subcmd.Options[0].IntValue())
		if err := b.store.RemoveRegexRule(context.Background(), id); err != nil {
			respondErr(s, i, t.FailedDelete)
			return
		}
		respondOK(s, i, t.RuleDeleted)
	}
}

func (b *Bot) cmdCSV(s *discordgo.Session, i *discordgo.InteractionCreate) {
	t := i18n.Get(b.getLocale(i))
	subcmd := i.ApplicationCommandData().Options[0]
	switch subcmd.Name {
	case "upload":
		attID := subcmd.Options[0].Value.(string)
		att := i.ApplicationCommandData().Resolved.Attachments[attID]

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
		var class, roleID string
		for _, o := range subcmd.Options {
			if o.Name == "class" {
				class = o.StringValue()
			} else if o.Name == "role" {
				roleID = o.RoleValue(nil, "").ID
			}
		}
		err := b.store.MapCSVClass(context.Background(), i.GuildID, class, roleID)
		if err != nil {
			respondErr(s, i, t.FailedMap)
			return
		}
		respondOK(s, i, fmt.Sprintf(t.ClassMappedFmt, class, roleID))
	}
}

func (b *Bot) handleComponent(s *discordgo.Session, i *discordgo.InteractionCreate) {
	t := i18n.Get(b.getLocale(i))
	switch i.MessageComponentData().CustomID {
	case "btn_verify_start":
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
	case "btn_enter_code":
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
	var localeStr string
	for _, o := range i.ApplicationCommandData().Options {
		if o.Name == "language" {
			localeStr = o.StringValue()
		}
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
	var count int64 = 3
	var window int64 = 30
	for _, o := range i.ApplicationCommandData().Options {
		if o.Name == "count" {
			count = o.IntValue()
		} else if o.Name == "window" {
			window = o.IntValue()
		}
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
	t := i18n.Get(b.getLocale(i))
	subcmd := i.ApplicationCommandData().Options[0]

	ctx := context.Background()
	cfg, ok, err := b.store.GetGuildConfig(ctx, i.GuildID)
	if err != nil {
		respondErr(s, i, t.FailedSave)
		return
	}
	if !ok {
		respondErr(s, i, i18n.Get(i18n.LocaleEN).ErrMissingConfig)
		return
	}

	switch subcmd.Name {
	case "set":
		var roleID string
		for _, o := range subcmd.Options {
			if o.Name == "role" {
				roleID = o.RoleValue(nil, "").ID
			}
		}
		cfg.DefaultRoleID = roleID
		if err := b.store.SaveGuildConfig(ctx, cfg); err != nil {
			respondErr(s, i, t.FailedSave)
			return
		}
		respondOK(s, i, fmt.Sprintf(t.VerifiedRoleSetFmt, roleID))

	case "view":
		if cfg.DefaultRoleID == "" {
			respondOK(s, i, t.VerifiedRoleNotSet)
			return
		}
		respondOK(s, i, fmt.Sprintf(t.VerifiedRoleViewFmt, cfg.DefaultRoleID))

	case "clear":
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

