package backup

import "time"

// BackupScope indicates whether a backup covers a single guild or multiple.
type BackupScope string

const (
	ScopeSingle  BackupScope = "single"
	ScopeMulti   BackupScope = "multi"
)

// BackupKind distinguishes manual from scheduled backups.
type BackupKind string

const (
	KindManual    BackupKind = "manual"
	KindScheduled BackupKind = "scheduled"
)

// ChannelEntry mirrors a subset of discordgo.Channel.
type ChannelEntry struct {
	ID            string `json:"id"`
	GuildID       string `json:"guild_id"`
	ParentID      string `json:"parent_id,omitempty"`
	Name          string `json:"name"`
	Type          int    `json:"type"`
	Position      int    `json:"position,omitempty"`
	Topic         string `json:"topic,omitempty"`
	NSFW          bool   `json:"nsfw,omitempty"`
	Bitrate       int    `json:"bitrate,omitempty"`
	RateLimit     int    `json:"rate_limit_per_user,omitempty"`
	PermissionOverwrites []PermissionEntry `json:"permission_overwrites,omitempty"`
}

// PermissionEntry mirrors a channel permission overwrite.
type PermissionEntry struct {
	ID    string `json:"id"`
	Type  int    `json:"type"`
	Allow int    `json:"allow"`
	Deny  int    `json:"deny"`
}

// RoleEntry mirrors a subset of discordgo.Role.
type RoleEntry struct {
	ID          string `json:"id"`
	GuildID     string `json:"guild_id"`
	Name        string `json:"name"`
	Permissions int    `json:"permissions"`
	Color       int    `json:"color"`
	Position    int    `json:"position"`
	Hoist       bool   `json:"hoist"`
	Mentionable bool   `json:"mentionable"`
}

// EmojiEntry mirrors a guild emoji.
type EmojiEntry struct {
	ID       string `json:"id"`
	Name     string `json:"name"`
	Animated bool   `json:"animated"`
	DataURL  string `json:"data_url,omitempty"` // local asset path
}

// BanEntry mirrors a guild ban.
type BanEntry struct {
	UserID string `json:"user_id"`
	Reason string `json:"reason,omitempty"`
}

// GuildSettingsEntry mirrors guild settings we can snapshot.
type GuildSettingsEntry struct {
	Name                        string `json:"name"`
	IconURL                     string `json:"icon_url,omitempty"`
	BannerURL                   string `json:"banner_url,omitempty"`
	VerificationLevel           int    `json:"verification_level"`
	ExplicitContentFilter       int    `json:"explicit_content_filter"`
	AFKChannelID                string `json:"afk_channel_id,omitempty"`
	AFKTimeout                  int    `json:"afk_timeout"`
	DefaultMessageNotifications int    `json:"default_message_notifications"`
	OptInEnabled                bool   `json:"opt_in_enabled"`
	WelcomeChannelID            string `json:"welcome_channel_id,omitempty"`
	SystemChannelID             string `json:"system_channel_id,omitempty"`
	SystemChannelFlags          int    `json:"system_channel_flags"`
	PreferredLocale             string `json:"preferred_locale"`
}

// BackupData is the full snapshot of a server.
type BackupData struct {
	Scope      BackupScope         `json:"scope"`
	GuildID    string              `json:"guild_id"`
	Guild      GuildSettingsEntry `json:"guild"`
	Categories []ChannelEntry     `json:"categories"`
	Channels   []ChannelEntry     `json:"channels"`
	Roles      []RoleEntry        `json:"roles"`
	Emojis     []EmojiEntry       `json:"emojis"`
	Bans       []BanEntry         `json:"bans"`
	CreatedAt  time.Time          `json:"created_at"`
}

// ScheduledBackupConfig controls automatic backups.
type ScheduledBackupConfig struct {
	GuildID    string    `json:"guild_id"`
	Enabled    bool      `json:"enabled"`
	Frequency  string    `json:"frequency"` // 12h, daily, weekly, biweekly, monthly, 3months, 6months
	TimeOfDay  string    `json:"time_of_day,omitempty"` // HH:MM for daily/weekly
	NextRun    time.Time `json:"next_run"`
	SlotCount  int       `json:"slot_count"` // always 3
}

// BackupRecord is the metadata stored in SQLite.
type BackupRecord struct {
	ID          int         `json:"id"`
	GuildID     string      `json:"guild_id"`
	Scope       BackupScope `json:"scope"`
	Kind        BackupKind  `json:"kind"`
	Filepath    string      `json:"filepath"`
	CreatedAt   time.Time   `json:"created_at"`
	ChannelCount int        `json:"channel_count"`
	RoleCount    int        `json:"role_count"`
	EmojiCount   int        `json:"emoji_count"`
	BanCount     int        `json:"ban_count"`
}