//! Guild backup system: capture, restore, JSON persistence, scheduled backups.
//!
//! Parity target: Go `internal/backup/{types,capture,restore,json_io,scheduler}.go`.

use crate::store::{BackupRecord, ScheduledBackup, Store, StoreError};
use chrono::{Datelike, SecondsFormat, TimeZone, Utc};
use serde::{Deserialize, Serialize};
use serenity::all::{
    AfkTimeout, ChannelType, CreateChannel, DefaultMessageNotificationLevel, EditRole, Emoji,
    ExplicitContentFilter, GuildChannel, GuildId, Http, PermissionOverwrite,
    PermissionOverwriteType, Permissions, Role, RoleId, UserId, VerificationLevel,
};
use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

#[derive(Debug, thiserror::Error)]
pub enum BackupError {
    #[error("store error: {0}")]
    Store(#[from] StoreError),
    #[error("discord error: {0}")]
    Discord(Box<serenity::Error>),
    #[error("http error: {0}")]
    Http(#[from] reqwest::Error),
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
    #[error("json error: {0}")]
    Json(#[from] serde_json::Error),
    #[error("invalid id: {0}")]
    Parse(#[from] std::num::ParseIntError),
}

impl From<serenity::Error> for BackupError {
    fn from(e: serenity::Error) -> Self {
        Self::Discord(Box::new(e))
    }
}

// ---------------------------------------------------------------------------
// Snapshot types (byte-compatible with the Go JSON)
// ---------------------------------------------------------------------------

fn is_default<T: Default + PartialEq>(t: &T) -> bool {
    *t == T::default()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BackupData {
    pub scope: String,
    pub guild_id: String,
    pub guild: GuildSettingsEntry,
    #[serde(default)]
    pub categories: Vec<ChannelEntry>,
    #[serde(default)]
    pub channels: Vec<ChannelEntry>,
    #[serde(default)]
    pub roles: Vec<RoleEntry>,
    #[serde(default)]
    pub emojis: Vec<EmojiEntry>,
    #[serde(default)]
    pub bans: Vec<BanEntry>,
    #[serde(default)]
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GuildSettingsEntry {
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub icon_url: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub banner_url: Option<String>,
    pub verification_level: u8,
    pub explicit_content_filter: u8,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub afk_channel_id: Option<String>,
    pub afk_timeout: u32,
    pub default_message_notifications: u8,
    #[serde(default, skip_serializing_if = "is_default")]
    pub opt_in_enabled: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub welcome_channel_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub system_channel_id: Option<String>,
    pub system_channel_flags: u64,
    pub preferred_locale: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChannelEntry {
    pub id: String,
    pub guild_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parent_id: Option<String>,
    pub name: String,
    #[serde(rename = "type")]
    pub channel_type: u8,
    #[serde(default, skip_serializing_if = "is_default")]
    pub position: i32,
    #[serde(default, skip_serializing_if = "is_default")]
    pub topic: String,
    #[serde(default, skip_serializing_if = "is_default")]
    pub nsfw: bool,
    #[serde(default, skip_serializing_if = "is_default")]
    pub bitrate: i32,
    #[serde(default, skip_serializing_if = "is_default")]
    pub rate_limit_per_user: i32,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub permission_overwrites: Vec<PermissionEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PermissionEntry {
    pub id: String,
    #[serde(rename = "type")]
    pub kind: u8,
    pub allow: i64,
    pub deny: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RoleEntry {
    pub id: String,
    pub guild_id: String,
    pub name: String,
    pub permissions: i64,
    pub color: i32,
    pub position: i32,
    pub hoist: bool,
    pub mentionable: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EmojiEntry {
    pub id: String,
    pub name: String,
    pub animated: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub data_url: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BanEntry {
    pub user_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
}

// ---------------------------------------------------------------------------
// Service
// ---------------------------------------------------------------------------

#[derive(Clone)]
pub struct BackupService {
    pub backup_dir: String,
    pub store: Store,
    pub http: Arc<Http>,
}

impl BackupService {
    pub fn new(backup_dir: String, store: Store, http: Arc<Http>) -> Self {
        Self {
            backup_dir,
            store,
            http,
        }
    }

    /// Parity: Go `CaptureGuild` — snapshot guild settings, channels, roles,
    /// emojis, and bans.
    pub async fn capture(&self, guild_id: &str, scope: &str) -> Result<BackupData, BackupError> {
        let guild_id = GuildId::new(guild_id.parse()?);
        let g = self.http.get_guild(guild_id).await?;

        let mut data = BackupData {
            scope: scope.to_string(),
            guild_id: g.id.to_string(),
            guild: GuildSettingsEntry {
                name: g.name.clone(),
                icon_url: g.icon_url(),
                banner_url: g.banner_url(),
                verification_level: verification_level_u8(g.verification_level),
                explicit_content_filter: explicit_content_filter_u8(g.explicit_content_filter),
                afk_channel_id: g
                    .afk_metadata
                    .as_ref()
                    .map(|a| a.afk_channel_id.to_string()),
                afk_timeout: g
                    .afk_metadata
                    .map(|a| afk_timeout_u32(a.afk_timeout))
                    .unwrap_or(0),
                default_message_notifications: default_notifications_u8(
                    g.default_message_notifications,
                ),
                opt_in_enabled: false,
                welcome_channel_id: None,
                system_channel_id: g.system_channel_id.map(|c| c.to_string()),
                system_channel_flags: g.system_channel_flags.bits(),
                preferred_locale: g.preferred_locale.clone(),
            },
            categories: Vec::new(),
            channels: Vec::new(),
            roles: Vec::new(),
            emojis: Vec::new(),
            bans: Vec::new(),
            created_at: Utc::now().to_rfc3339_opts(SecondsFormat::Secs, true),
        };

        for ch in guild_id.channels(&self.http).await?.values() {
            let entry = channel_entry(ch);
            if ch.kind == ChannelType::Category {
                data.categories.push(entry);
            } else {
                data.channels.push(entry);
            }
        }

        for r in guild_id.roles(&self.http).await?.values() {
            data.roles.push(role_entry(r));
        }

        for e in guild_id.emojis(&self.http).await? {
            data.emojis.push(emoji_entry(&e));
        }

        for b in guild_id.bans(&self.http, None, None).await? {
            data.bans.push(BanEntry {
                user_id: b.user.id.to_string(),
                reason: b.reason.clone(),
            });
        }

        Ok(data)
    }

    /// Parity: Go `DownloadEmojiAssets` — fetch emoji images into `assets_dir`
    /// and rewrite `data_url` to the relative path.
    pub async fn download_emoji_assets(
        &self,
        data: &mut BackupData,
        assets_dir: &str,
    ) -> Result<(), BackupError> {
        std::fs::create_dir_all(assets_dir)?;
        for e in &mut data.emojis {
            let Some(url) = e.data_url.clone() else {
                continue;
            };
            if url.is_empty() {
                continue;
            }
            let ext = if url.contains(".gif") || e.animated {
                ".gif"
            } else {
                ".png"
            };
            let rel = format!("emoji_{}_{}{}", e.id, e.name, ext);
            let local = Path::new(assets_dir).join(&rel);
            if local.exists() {
                e.data_url = Some(rel);
                continue;
            }
            let bytes = reqwest::get(&url)
                .await?
                .error_for_status()?
                .bytes()
                .await?
                .to_vec();
            std::fs::write(&local, bytes)?;
            e.data_url = Some(rel);
        }
        Ok(())
    }

    /// Parity: Go `RestoreGuild` — create missing categories/channels/roles and
    /// apply bans (existing items skipped; ban errors ignored).
    pub async fn restore(
        &self,
        data: &BackupData,
        target_guild_id: &str,
    ) -> Result<(), BackupError> {
        let target = GuildId::new(target_guild_id.parse()?);
        let existing_channels = target.channels(&self.http).await?;
        let existing_roles = target.roles(&self.http).await?;

        // 1. Categories (skip by name).
        let mut created_categories: HashMap<String, serenity::all::ChannelId> = existing_channels
            .iter()
            .filter(|(_, c)| c.kind == ChannelType::Category)
            .map(|(id, c)| (c.name.clone(), *id))
            .collect();
        for cat in &data.categories {
            if created_categories.contains_key(&cat.name) {
                continue;
            }
            let created = target
                .create_channel(
                    &self.http,
                    CreateChannel::new(&cat.name).kind(ChannelType::Category),
                )
                .await?;
            created_categories.insert(cat.name.clone(), created.id);
        }

        // 2. Channels (skip by name + type).
        for ch in &data.channels {
            let skip = existing_channels
                .values()
                .any(|ec| ec.name == ch.name && channel_type_to_u8(ec.kind) == ch.channel_type);
            if skip {
                continue;
            }
            let mut builder = CreateChannel::new(&ch.name)
                .kind(channel_type_from_u8(ch.channel_type))
                .position(ch.position.max(0) as u16);
            if !ch.topic.is_empty() {
                builder = builder.topic(&ch.topic);
            }
            if ch.bitrate > 0 {
                builder = builder.bitrate(ch.bitrate as u32);
            }
            if ch.rate_limit_per_user > 0 {
                builder = builder.rate_limit_per_user(ch.rate_limit_per_user as u16);
            }
            if let Some(parent) = &ch.parent_id
                && let Ok(parent_id) = parent.parse::<u64>()
            {
                builder = builder.category(serenity::all::ChannelId::new(parent_id));
            }
            if !ch.permission_overwrites.is_empty() {
                builder = builder.permissions(ch.permission_overwrites.iter().map(|po| {
                    let kind = if po.kind == 0 {
                        PermissionOverwriteType::Role(RoleId::new(po.id.parse().unwrap_or(0)))
                    } else {
                        PermissionOverwriteType::Member(UserId::new(po.id.parse().unwrap_or(0)))
                    };
                    PermissionOverwrite {
                        allow: Permissions::from_bits_truncate(po.allow as u64),
                        deny: Permissions::from_bits_truncate(po.deny as u64),
                        kind,
                    }
                }));
            }
            target.create_channel(&self.http, builder).await?;
        }

        // 3. Roles (skip @everyone + case-insensitive name duplicates).
        for r in &data.roles {
            if r.name == "@everyone" {
                continue;
            }
            let skip = existing_roles
                .values()
                .any(|er| er.name.eq_ignore_ascii_case(&r.name));
            if skip {
                continue;
            }
            target
                .create_role(
                    &self.http,
                    EditRole::new()
                        .name(&r.name)
                        .permissions(Permissions::from_bits_truncate(r.permissions as u64))
                        .colour(r.color as u32)
                        .hoist(r.hoist)
                        .mentionable(r.mentionable),
                )
                .await?;
        }

        // 4. Bans (ignore errors — may already exist).
        for b in &data.bans {
            if let Ok(uid) = b.user_id.parse::<u64>() {
                let _ = target
                    .ban_with_reason(
                        &self.http,
                        UserId::new(uid),
                        0,
                        b.reason.as_deref().unwrap_or_default(),
                    )
                    .await;
            }
        }

        Ok(())
    }

    /// Write the snapshot to `{backup_dir}/{prefix}_{guild}_{ts}.json`.
    pub fn write_backup_file(
        &self,
        prefix: &str,
        guild_id: &str,
        data: &BackupData,
    ) -> Result<String, BackupError> {
        std::fs::create_dir_all(&self.backup_dir)?;
        let ts = Utc::now().format("%Y%m%d_%H%M%S");
        let path = Path::new(&self.backup_dir).join(format!("{prefix}_{guild_id}_{ts}.json"));
        write_json(&path, data)?;
        Ok(path.to_string_lossy().into_owned())
    }

    /// Parity: Go `rotateScheduled` — keep only the 3 newest scheduled backups.
    pub fn rotate_scheduled(&self, guild_id: &str) -> Result<(), BackupError> {
        let records = self.store.list_backups(Some(guild_id), Some("scheduled"))?;
        for record in records.iter().skip(3) {
            let _ = std::fs::remove_file(&record.filepath);
            self.store.delete_backup(record.id)?;
        }
        Ok(())
    }

    /// Run one scheduled backup for a guild (capture, assets, rotate, write,
    /// record, next-run). Parity: Go `Scheduler.runScheduled`.
    pub async fn run_scheduled(&self, cfg: &ScheduledBackup) -> Result<(), BackupError> {
        let now = now_unix_secs();
        let mut data = self.capture(&cfg.guild_id, "single").await?;

        let assets_dir = Path::new(&self.backup_dir)
            .join("assets")
            .join(format!("scheduled_{}", unix_nanos()));
        if let Err(e) = self
            .download_emoji_assets(&mut data, &assets_dir.to_string_lossy())
            .await
        {
            tracing::warn!(%e, guild = %cfg.guild_id, "scheduled emoji download failed");
        }
        if let Err(e) = self.rotate_scheduled(&cfg.guild_id) {
            tracing::warn!(%e, guild = %cfg.guild_id, "scheduled rotation failed");
        }

        let file_path = self.write_backup_file("scheduled", &cfg.guild_id, &data)?;
        let record = BackupRecord {
            id: 0,
            guild_id: cfg.guild_id.clone(),
            scope: "single".into(),
            kind: "scheduled".into(),
            filepath: file_path.clone(),
            created_at: now,
            channel_count: data.channels.len() as i64,
            role_count: data.roles.len() as i64,
            emoji_count: data.emojis.len() as i64,
            ban_count: data.bans.len() as i64,
        };
        self.store.save_backup(&record)?;

        let next = Self::next_run(now, &cfg.frequency, &cfg.time_of_day);
        let mut next_cfg = cfg.clone();
        next_cfg.next_run = next;
        self.store.save_scheduled_config(&next_cfg)?;
        tracing::info!(guild = %cfg.guild_id, "scheduled backup created");
        Ok(())
    }

    /// Parity: Go `Scheduler.check` — run due scheduled backups (60s polled).
    pub async fn check_scheduled(&self) -> Result<(), BackupError> {
        let now = now_unix_secs();
        for cfg in self.store.list_scheduled_backups()? {
            if cfg.enabled && now >= cfg.next_run {
                let backup = self.clone();
                tokio::spawn(async move {
                    if let Err(e) = backup.run_scheduled(&cfg).await {
                        tracing::error!(%e, guild = %cfg.guild_id, "scheduled backup failed");
                    }
                });
            }
        }
        Ok(())
    }

    /// Parity: Go `FrequencyInterval` (seconds).
    pub fn frequency_interval(frequency: &str) -> i64 {
        match frequency {
            "12h" => 12 * 3600,
            "daily" => 24 * 3600,
            "weekly" => 7 * 24 * 3600,
            "biweekly" => 14 * 24 * 3600,
            "monthly" => 30 * 24 * 3600,
            "3months" => 90 * 24 * 3600,
            "6months" => 180 * 24 * 3600,
            _ => 24 * 3600,
        }
    }

    /// Compute the next scheduled run. Parity: Go `runScheduled` next-run math —
    /// time-of-day (`HH:MM` UTC) only applies to daily/weekly.
    pub fn next_run(now: i64, frequency: &str, time_of_day: &str) -> i64 {
        if !time_of_day.is_empty()
            && (frequency == "daily" || frequency == "weekly")
            && let Some((h, m)) = parse_hhmm(time_of_day)
        {
            let today = today_at_hhmm(now, h, m);
            if today > now {
                return today;
            }
            return today + 24 * 3600;
        }
        now + Self::frequency_interval(frequency)
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Spawn the scheduled-backup loop (60s tick, Go parity).
pub fn spawn_scheduler(backup: Arc<BackupService>) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(Duration::from_secs(60));
        interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
        loop {
            interval.tick().await;
            if let Err(e) = backup.check_scheduled().await {
                tracing::error!(%e, "scheduled backup check failed");
            }
        }
    })
}

fn write_json(path: &Path, data: &BackupData) -> Result<(), BackupError> {
    let f = std::fs::File::create(path)?;
    let mut writer = std::io::BufWriter::new(f);
    serde_json::to_writer_pretty(&mut writer, data)?;
    Ok(())
}

pub fn read_json(path: &str) -> Result<BackupData, BackupError> {
    let bytes = std::fs::read(path)?;
    Ok(serde_json::from_slice(&bytes)?)
}

fn channel_entry(ch: &GuildChannel) -> ChannelEntry {
    ChannelEntry {
        id: ch.id.to_string(),
        guild_id: String::new(),
        parent_id: ch.parent_id.map(|p| p.to_string()),
        name: ch.name.clone(),
        channel_type: channel_type_to_u8(ch.kind),
        position: ch.position as i32,
        topic: ch.topic.clone().unwrap_or_default(),
        nsfw: ch.nsfw,
        bitrate: ch.bitrate.map(|b| b as i32).unwrap_or(0),
        rate_limit_per_user: ch.rate_limit_per_user.map(|r| r as i32).unwrap_or(0),
        permission_overwrites: ch
            .permission_overwrites
            .iter()
            .map(|po| {
                let (id, kind) = match po.kind {
                    PermissionOverwriteType::Role(r) => (r.to_string(), 0),
                    PermissionOverwriteType::Member(m) => (m.to_string(), 1),
                    _ => (String::new(), 0),
                };
                PermissionEntry {
                    id,
                    kind,
                    allow: po.allow.bits() as i64,
                    deny: po.deny.bits() as i64,
                }
            })
            .collect(),
    }
}

fn role_entry(r: &Role) -> RoleEntry {
    RoleEntry {
        id: r.id.to_string(),
        guild_id: String::new(),
        name: r.name.clone(),
        permissions: r.permissions.bits() as i64,
        color: r.colour.0 as i32,
        position: r.position as i32,
        hoist: r.hoist,
        mentionable: r.mentionable,
    }
}

fn emoji_entry(e: &Emoji) -> EmojiEntry {
    EmojiEntry {
        id: e.id.to_string(),
        name: e.name.clone(),
        animated: e.animated,
        data_url: Some(emoji_url(e)),
    }
}

/// Parity: Go `emojiURL`.
fn emoji_url(e: &Emoji) -> String {
    if e.animated {
        format!("https://cdn.discordapp.com/emojis/{}.gif?size=128", e.id)
    } else {
        format!("https://cdn.discordapp.com/emojis/{}.png?size=128", e.id)
    }
}

fn channel_type_to_u8(kind: ChannelType) -> u8 {
    match kind {
        ChannelType::Text => 0,
        ChannelType::Private => 1,
        ChannelType::Voice => 2,
        ChannelType::GroupDm => 3,
        ChannelType::Category => 4,
        ChannelType::News => 5,
        ChannelType::NewsThread => 10,
        ChannelType::PublicThread => 11,
        ChannelType::PrivateThread => 12,
        ChannelType::Stage => 13,
        ChannelType::Directory => 14,
        ChannelType::Forum => 15,
        ChannelType::Unknown(u) => u,
        _ => 0,
    }
}

fn channel_type_from_u8(v: u8) -> ChannelType {
    match v {
        0 => ChannelType::Text,
        1 => ChannelType::Private,
        2 => ChannelType::Voice,
        3 => ChannelType::GroupDm,
        4 => ChannelType::Category,
        5 => ChannelType::News,
        10 => ChannelType::NewsThread,
        11 => ChannelType::PublicThread,
        12 => ChannelType::PrivateThread,
        13 => ChannelType::Stage,
        14 => ChannelType::Directory,
        15 => ChannelType::Forum,
        other => ChannelType::Unknown(other),
    }
}

fn verification_level_u8(v: VerificationLevel) -> u8 {
    match v {
        VerificationLevel::None => 0,
        VerificationLevel::Low => 1,
        VerificationLevel::Medium => 2,
        VerificationLevel::High => 3,
        VerificationLevel::Higher => 4,
        VerificationLevel::Unknown(u) => u,
        _ => 0,
    }
}

fn explicit_content_filter_u8(v: ExplicitContentFilter) -> u8 {
    match v {
        ExplicitContentFilter::None => 0,
        ExplicitContentFilter::WithoutRole => 1,
        ExplicitContentFilter::All => 2,
        ExplicitContentFilter::Unknown(u) => u,
        _ => 0,
    }
}

fn default_notifications_u8(v: DefaultMessageNotificationLevel) -> u8 {
    match v {
        DefaultMessageNotificationLevel::All => 0,
        DefaultMessageNotificationLevel::Mentions => 1,
        DefaultMessageNotificationLevel::Unknown(u) => u,
        _ => 0,
    }
}

fn afk_timeout_u32(v: AfkTimeout) -> u32 {
    match v {
        AfkTimeout::OneMinute => 60,
        AfkTimeout::FiveMinutes => 300,
        AfkTimeout::FifteenMinutes => 900,
        AfkTimeout::ThirtyMinutes => 1800,
        AfkTimeout::OneHour => 3600,
        AfkTimeout::Unknown(u) => u as u32,
        _ => 0,
    }
}

fn parse_hhmm(s: &str) -> Option<(u32, u32)> {
    let mut parts = s.split(':');
    let h = parts.next()?.parse().ok()?;
    let m = parts.next()?.parse().ok()?;
    Some((h, m))
}

fn today_at_hhmm(now: i64, h: u32, m: u32) -> i64 {
    let dt = Utc.timestamp_opt(now, 0).single().unwrap_or_default();
    Utc.with_ymd_and_hms(dt.year(), dt.month(), dt.day(), h, m, 0)
        .single()
        .map(|t| t.timestamp())
        .unwrap_or(now)
}

pub(crate) fn now_unix_secs() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

pub(crate) fn unix_nanos() -> u128 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_data() -> BackupData {
        BackupData {
            scope: "single".into(),
            guild_id: "100".into(),
            guild: GuildSettingsEntry {
                name: "Test Guild".into(),
                icon_url: None,
                banner_url: None,
                verification_level: 1,
                explicit_content_filter: 2,
                afk_channel_id: None,
                afk_timeout: 300,
                default_message_notifications: 0,
                opt_in_enabled: false,
                welcome_channel_id: None,
                system_channel_id: None,
                system_channel_flags: 0,
                preferred_locale: "en-US".into(),
            },
            categories: vec![ChannelEntry {
                id: "cat".into(),
                guild_id: String::new(),
                parent_id: None,
                name: "Main".into(),
                channel_type: 4,
                position: 0,
                topic: String::new(),
                nsfw: false,
                bitrate: 0,
                rate_limit_per_user: 0,
                permission_overwrites: vec![],
            }],
            channels: vec![],
            roles: vec![],
            emojis: vec![],
            bans: vec![],
            created_at: "2026-01-01T00:00:00Z".into(),
        }
    }

    #[test]
    fn json_round_trip() {
        let data = sample_data();
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("backup.json");
        write_json(&path, &data).unwrap();
        let loaded = read_json(path.to_str().unwrap()).unwrap();
        assert_eq!(loaded.guild.name, "Test Guild");
        assert_eq!(loaded.categories[0].channel_type, 4);
        assert_eq!(loaded.created_at, "2026-01-01T00:00:00Z");
        assert!(loaded.categories[0].permission_overwrites.is_empty());
    }

    #[test]
    fn json_omits_empty_permission_overwrites() {
        let data = sample_data();
        let json = serde_json::to_value(&data).unwrap();
        assert!(json["categories"][0].get("permission_overwrites").is_none());
        assert!(json["categories"][0].get("topic").is_none());
    }

    #[test]
    fn frequency_intervals() {
        assert_eq!(BackupService::frequency_interval("12h"), 12 * 3600);
        assert_eq!(BackupService::frequency_interval("daily"), 24 * 3600);
        assert_eq!(BackupService::frequency_interval("weekly"), 7 * 24 * 3600);
        assert_eq!(
            BackupService::frequency_interval("biweekly"),
            14 * 24 * 3600
        );
        assert_eq!(BackupService::frequency_interval("monthly"), 30 * 24 * 3600);
        assert_eq!(BackupService::frequency_interval("3months"), 90 * 24 * 3600);
        assert_eq!(
            BackupService::frequency_interval("6months"),
            180 * 24 * 3600
        );
        assert_eq!(BackupService::frequency_interval("bogus"), 24 * 3600);
    }

    #[test]
    fn next_run_interval_based() {
        let now = 1_000_000;
        assert_eq!(BackupService::next_run(now, "daily", ""), now + 86400);
        assert_eq!(BackupService::next_run(now, "weekly", ""), now + 7 * 86400);
    }

    #[test]
    fn next_run_time_of_day() {
        // now = 2025-01-01 13:00 UTC
        let now = 1_735_736_400;
        // 08:00 today (1_735_718_400) is past → tomorrow 08:00 (1_735_804_800).
        assert_eq!(
            BackupService::next_run(now, "daily", "08:00"),
            1_735_804_800
        );
        // 20:00 today (1_735_761_600) is ahead → today.
        assert_eq!(
            BackupService::next_run(now, "daily", "20:00"),
            1_735_761_600
        );
        // time-of-day ignored for non-daily/weekly.
        assert_eq!(
            BackupService::next_run(now, "12h", "08:00"),
            now + 12 * 3600
        );
    }

    #[test]
    fn channel_type_conversions() {
        for v in [0u8, 4, 5, 10, 15, 99] {
            assert_eq!(channel_type_to_u8(channel_type_from_u8(v)), v);
        }
    }

    #[test]
    fn permission_overwrite_conversion_matches_go_json() {
        let po = PermissionEntry {
            id: "123".into(),
            kind: 0,
            allow: 8,
            deny: 0,
        };
        assert_eq!(serde_json::to_value(&po).unwrap()["type"], 0);
        assert_eq!(serde_json::to_value(&po).unwrap()["allow"], 8);
    }
}
