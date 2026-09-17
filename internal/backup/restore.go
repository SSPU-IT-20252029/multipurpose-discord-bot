package backup

import (
	"fmt"
	"strings"

	"github.com/bwmarrin/discordgo"
)

// RestoreGuild recreates the structure described by data into targetGuildID.
// Existing items (categories, channels, roles) with the same name are skipped;
// missing items are created. Bans are applied for users not already banned.
func RestoreGuild(s *discordgo.Session, data *BackupData, targetGuildID string) error {
	existingChannels, err := s.GuildChannels(targetGuildID)
	if err != nil {
		return fmt.Errorf("listing target channels: %w", err)
	}
	existingRoles, err := s.GuildRoles(targetGuildID)
	if err != nil {
		return fmt.Errorf("listing target roles: %w", err)
	}

	// 1. Create categories first.
	createdCategories := map[string]string{}
	for _, ch := range existingChannels {
		if ch.Type == discordgo.ChannelTypeGuildCategory {
			createdCategories[ch.Name] = ch.ID
		}
	}

	for _, cat := range data.Categories {
		if _, ok := createdCategories[cat.Name]; ok {
			continue
		}
		created, err := s.GuildChannelCreateComplex(targetGuildID, discordgo.GuildChannelCreateData{
			Name:     cat.Name,
			ParentID: cat.ParentID,
			Type:     discordgo.ChannelTypeGuildCategory,
		})
		if err != nil {
			return fmt.Errorf("creating category %s: %w", cat.Name, err)
		}
		createdCategories[cat.Name] = created.ID
	}

	// 2. Create channels (text/voice).
	for _, ch := range data.Channels {
		skip := false
		for _, ec := range existingChannels {
			if ec.Name == ch.Name && ec.Type == discordgo.ChannelType(ch.Type) {
				skip = true
				break
			}
		}
		if skip {
			continue
		}
		_, err := s.GuildChannelCreateComplex(targetGuildID, discordgo.GuildChannelCreateData{
			Name:                 ch.Name,
			ParentID:             ch.ParentID,
			Type:                 discordgo.ChannelType(ch.Type),
			Topic:                ch.Topic,
			Position:             ch.Position,
			RateLimitPerUser:     ch.RateLimit,
			PermissionOverwrites: convertOverwrites(ch.PermissionOverwrites),
		})
		if err != nil {
			return fmt.Errorf("creating channel %s: %w", ch.Name, err)
		}
	}

	// 3. Create roles (skip @everyone).
	for _, r := range data.Roles {
		if r.Name == "@everyone" {
			continue
		}
		skip := false
		for _, er := range existingRoles {
			if strings.EqualFold(er.Name, r.Name) {
				skip = true
				break
			}
		}
		if skip {
			continue
		}
		_, err := s.GuildRoleCreate(targetGuildID, &discordgo.RoleParams{
			Name:        r.Name,
			Permissions: int64Ptr(r.Permissions),
			Color:       &r.Color,
			Hoist:       &r.Hoist,
			Mentionable: &r.Mentionable,
		})
		if err != nil {
			return fmt.Errorf("creating role %s: %w", r.Name, err)
		}
	}

	// 4. Apply bans.
	for _, b := range data.Bans {
		err := s.GuildBanCreateWithReason(targetGuildID, b.UserID, b.Reason, 0)
		if err != nil {
			// Ban may already exist; ignore.
			_ = err
		}
	}

	return nil
}

func convertOverwrites(in []PermissionEntry) []*discordgo.PermissionOverwrite {
	out := make([]*discordgo.PermissionOverwrite, 0, len(in))
	for _, po := range in {
		out = append(out, &discordgo.PermissionOverwrite{
			ID:    po.ID,
			Type:  discordgo.PermissionOverwriteType(po.Type),
			Allow: int64(po.Allow),
			Deny:  int64(po.Deny),
		})
	}
	return out
}

func int64Ptr(n int) *int64 {
	v := int64(n)
	return &v
}