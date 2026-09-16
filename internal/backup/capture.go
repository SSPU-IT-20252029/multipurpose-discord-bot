package backup

import (
	"fmt"
	"io"
	"net/http"
	"os"
	"path/filepath"
	"strings"
	"time"

	"github.com/bwmarrin/discordgo"
)

// CaptureGuild fetches the structure of a guild and its assets.
func CaptureGuild(s *discordgo.Session, guildID string) (*BackupData, error) {
	g, err := s.Guild(guildID)
	if err != nil {
		return nil, fmt.Errorf("fetching guild: %w", err)
	}

	data := &BackupData{
		Scope:   ScopeSingle,
		GuildID: g.ID,
		Guild: GuildSettingsEntry{
			Name:                        g.Name,
			IconURL:                     g.IconURL("256"),
			BannerURL:                   g.BannerURL("256"),
			VerificationLevel:           int(g.VerificationLevel),
			ExplicitContentFilter:       int(g.ExplicitContentFilter),
			AFKChannelID:                g.AfkChannelID,
			AFKTimeout:                  int(g.AfkTimeout),
			DefaultMessageNotifications: int(g.DefaultMessageNotifications),
			SystemChannelID:             g.SystemChannelID,
			SystemChannelFlags:          int(g.SystemChannelFlags),
			PreferredLocale:             g.PreferredLocale,
		},
		CreatedAt: time.Now().UTC(),
	}

	channels, err := s.GuildChannels(guildID)
	if err != nil {
		return nil, fmt.Errorf("fetching channels: %w", err)
	}

	for _, ch := range channels {
		entry := ChannelEntry{
			ID:            ch.ID,
			ParentID:      ch.ParentID,
			Name:          ch.Name,
			Type:          int(ch.Type),
			Position:      ch.Position,
			Topic:         ch.Topic,
			NSFW:          ch.NSFW,
			Bitrate:       ch.Bitrate,
			RateLimit:     ch.RateLimitPerUser,
		}
		for _, po := range ch.PermissionOverwrites {
			entry.PermissionOverwrites = append(entry.PermissionOverwrites, PermissionEntry{
				ID:    po.ID,
				Type:  int(po.Type),
				Allow: int(po.Allow),
				Deny:  int(po.Deny),
			})
		}
		if ch.Type == discordgo.ChannelTypeGuildCategory {
			data.Categories = append(data.Categories, entry)
		} else {
			data.Channels = append(data.Channels, entry)
		}
	}

	roles, err := s.GuildRoles(guildID)
	if err != nil {
		return nil, fmt.Errorf("fetching roles: %w", err)
	}
	for _, r := range roles {
		data.Roles = append(data.Roles, RoleEntry{
			ID:          r.ID,
			Name:        r.Name,
			Permissions: int(r.Permissions),
			Color:       r.Color,
			Position:    r.Position,
			Hoist:       r.Hoist,
			Mentionable: r.Mentionable,
		})
	}

	emojis, err := s.GuildEmojis(guildID)
	if err != nil {
		return nil, fmt.Errorf("fetching emojis: %w", err)
	}
	for _, e := range emojis {
		data.Emojis = append(data.Emojis, EmojiEntry{
			ID:       e.ID,
			Name:     e.Name,
			Animated: e.Animated,
			DataURL:  emojiURL(e),
		})
	}

	bans, err := s.GuildBans(guildID, 1000, "", "")
	if err != nil {
		return nil, fmt.Errorf("fetching bans: %w", err)
	}
	for _, b := range bans {
		data.Bans = append(data.Bans, BanEntry{
			UserID: b.User.ID,
			Reason: b.Reason,
		})
	}

	return data, nil
}

func emojiURL(e *discordgo.Emoji) string {
	if e.Animated {
		return fmt.Sprintf("https://cdn.discordapp.com/emojis/%s.gif?size=128", e.ID)
	}
	return fmt.Sprintf("https://cdn.discordapp.com/emojis/%s.png?size=128", e.ID)
}

// DownloadEmojiAssets fetches emoji images into dir and rewrites
// DataURL to the local relative path.
func DownloadEmojiAssets(data *BackupData, dir string) error {
	if err := os.MkdirAll(dir, 0o755); err != nil {
		return fmt.Errorf("creating asset dir: %w", err)
	}
	for i, e := range data.Emojis {
		if e.DataURL == "" {
			continue
		}
		ext := ".png"
		if strings.Contains(e.DataURL, ".gif") || e.Animated {
			ext = ".gif"
		}
		rel := fmt.Sprintf("emoji_%s_%s%s", e.ID, e.Name, ext)
		local := filepath.Join(dir, rel)
		if _, err := os.Stat(local); err == nil {
			data.Emojis[i].DataURL = rel
			continue
		}
		if err := downloadFile(e.DataURL, local); err != nil {
			return fmt.Errorf("downloading emoji %s: %w", e.ID, err)
		}
		data.Emojis[i].DataURL = rel
	}
	return nil
}

func downloadFile(url, dest string) error {
	resp, err := http.Get(url)
	if err != nil {
		return err
	}
	defer resp.Body.Close()
	if resp.StatusCode != http.StatusOK {
		return fmt.Errorf("unexpected status %d", resp.StatusCode)
	}
	f, err := os.Create(dest)
	if err != nil {
		return err
	}
	defer f.Close()
	_, err = io.Copy(f, resp.Body)
	return err
}