package backup

import (
	"context"
	"fmt"
	"log"
	"os"
	"path/filepath"
	"strings"
	"sync"
	"time"

	"github.com/bwmarrin/discordgo"

	"sspu-verifier/internal/store"
)

// Frequency intervals for scheduled backups.
const (
	Freq12Hours   = "12h"
	FreqDaily     = "daily"
	FreqWeekly    = "weekly"
	FreqBiweekly  = "biweekly"
	FreqMonthly   = "monthly"
	Freq3Months   = "3months"
	Freq6Months   = "6months"
)

// FrequencyInterval maps a frequency string to a time.Duration.
func FrequencyInterval(f string) time.Duration {
	switch f {
	case Freq12Hours:
		return 12 * time.Hour
	case FreqDaily:
		return 24 * time.Hour
	case FreqWeekly:
		return 7 * 24 * time.Hour
	case FreqBiweekly:
		return 14 * 24 * time.Hour
	case FreqMonthly:
		return 30 * 24 * time.Hour
	case Freq3Months:
		return 90 * 24 * time.Hour
	case Freq6Months:
		return 180 * 24 * time.Hour
	default:
		return 24 * time.Hour
	}
}

// ValidFrequencies lists all accepted frequency strings.
var ValidFrequencies = []string{Freq12Hours, FreqDaily, FreqWeekly, FreqBiweekly, FreqMonthly, Freq3Months, Freq6Months}

// Scheduler runs periodic backups for configured guilds.
type Scheduler struct {
	s         *discordgo.Session
	store     *store.Store
	backupDir string
	stop      chan struct{}
	mu        sync.Mutex
}

// NewScheduler creates a scheduler.
func NewScheduler(s *discordgo.Session, st *store.Store, backupDir string) *Scheduler {
	return &Scheduler{
		s:         s,
		store:     st,
		backupDir: backupDir,
		stop:      make(chan struct{}),
	}
}

// BackupDir returns the configured backup directory.
func (sc *Scheduler) BackupDir() string {
	return sc.backupDir
}

// Start begins the scheduler loop.
func (sc *Scheduler) Start() {
	go sc.loop()
}

// Stop halts the scheduler.
func (sc *Scheduler) Stop() {
	sc.mu.Lock()
	defer sc.mu.Unlock()
	select {
	case <-sc.stop:
		return
	default:
		close(sc.stop)
	}
}

func (sc *Scheduler) loop() {
	ticker := time.NewTicker(1 * time.Minute)
	defer ticker.Stop()
	for {
		select {
		case <-sc.stop:
			return
		case now := <-ticker.C:
			sc.check(now)
		}
	}
}

func (sc *Scheduler) check(now time.Time) {
	ctx := context.Background()
	configs, err := sc.store.ListScheduledBackups(ctx)
	if err != nil {
		log.Printf("[backup] scheduler: %v", err)
		return
	}
	for _, c := range configs {
		if !c.Enabled || now.Before(c.NextRun) {
			continue
		}
		go sc.runScheduled(ctx, now, c)
	}
}

func (sc *Scheduler) runScheduled(ctx context.Context, now time.Time, c store.ScheduledBackupConfig) {
	data, err := CaptureGuild(sc.s, c.GuildID)
	if err != nil {
		log.Printf("[backup] scheduled capture failed for guild %s: %v", c.GuildID, err)
		return
	}
	data.Scope = ScopeSingle

	assetsDir := filepath.Join(sc.backupDir, "assets", fmt.Sprintf("scheduled_%d", time.Now().UnixNano()))
	if err := DownloadEmojiAssets(data, assetsDir); err != nil {
		log.Printf("[backup] scheduled emoji download for guild %s: %v", c.GuildID, err)
	}

	// Rotate: keep only the 3 most recent scheduled backups for this guild.
	if err := sc.rotateScheduled(ctx, c.GuildID); err != nil {
		log.Printf("[backup] rotation failed: %v", err)
	}

	filePath := filepath.Join(sc.backupDir, fmt.Sprintf("scheduled_%s_%s.json", c.GuildID, time.Now().Format("20060102_150405")))
	if err := os.MkdirAll(sc.backupDir, 0o755); err != nil {
		log.Printf("[backup] mkdir failed: %v", err)
		return
	}
	if err := WriteJSON(filePath, data); err != nil {
		log.Printf("[backup] write failed: %v", err)
		return
	}

	rec := store.BackupRecord{
		GuildID:      c.GuildID,
		Scope:        string(ScopeSingle),
		Kind:         string(KindScheduled),
		Filepath:     filePath,
		CreatedAt:    time.Now().UTC(),
		ChannelCount: len(data.Channels),
		RoleCount:    len(data.Roles),
		EmojiCount:   len(data.Emojis),
		BanCount:     len(data.Bans),
	}
	if _, err := sc.store.SaveBackup(ctx, rec); err != nil {
		log.Printf("[backup] save record failed: %v", err)
		return
	}

	// Compute next run.
	next := time.Now().Add(FrequencyInterval(c.Frequency))
	if c.TimeOfDay != "" && (c.Frequency == FreqDaily || c.Frequency == FreqWeekly) {
		parts := strings.Split(c.TimeOfDay, ":")
		if len(parts) == 2 {
			var h, m int
			fmt.Sscanf(parts[0], "%d", &h)
			fmt.Sscanf(parts[1], "%d", &m)
			today := time.Date(now.Year(), now.Month(), now.Day(), h, m, 0, 0, time.UTC)
			if today.Before(now) {
				today = today.Add(24 * time.Hour)
			}
			next = today
		}
	}
	c.NextRun = next
	if err := sc.store.SaveScheduledConfig(ctx, c); err != nil {
		log.Printf("[backup] update next_run failed: %v", err)
	}
	log.Printf("[backup] scheduled backup created for guild %s", c.GuildID)
}

// rotateScheduled deletes scheduled backups beyond the 3 most recent.
func (sc *Scheduler) rotateScheduled(ctx context.Context, guildID string) error {
	records, err := sc.store.ListBackups(ctx, guildID, string(KindScheduled))
	if err != nil {
		return err
	}
	// ListBackups returns newest first; keep the first 3.
	for i, r := range records {
		if i >= 3 {
			_ = os.Remove(r.Filepath)
			if err := sc.store.DeleteBackup(ctx, r.ID); err != nil {
				return err
			}
		}
	}
	return nil
}