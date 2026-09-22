package config

import (
	"fmt"
	"os"
	"regexp"

	"gopkg.in/yaml.v3"
)

type Config struct {
	Discord Discord `yaml:"discord"`
	Email   Email   `yaml:"email"`
	Storage Storage `yaml:"storage"`
}

type Discord struct {
	Token string `yaml:"token"`
}

type Email struct {
	APIKey string `yaml:"api_key"`
	From   string `yaml:"from"`
}

type Storage struct {
	DSN       string `yaml:"dsn"`
	BackupDir string `yaml:"backup_dir"`
}

var envRe = regexp.MustCompile(`\$\{([A-Za-z_][A-Za-z0-9_]*)\}`)

func Load(path string) (*Config, error) {
	raw, err := os.ReadFile(path)
	if err != nil {
		return nil, fmt.Errorf("reading configuration: %w", err)
	}
	expanded := envRe.ReplaceAllFunc(raw, func(m []byte) []byte {
		name := envRe.FindSubmatch(m)[1]
		return []byte(os.Getenv(string(name)))
	})
	var cfg Config
	if err := yaml.Unmarshal(expanded, &cfg); err != nil {
		return nil, fmt.Errorf("parsing configuration: %w", err)
	}
	applyDefaults(&cfg)
	if err := validate(&cfg); err != nil {
		return nil, err
	}
	return &cfg, nil
}

func applyDefaults(cfg *Config) {
	if cfg.Storage.DSN == "" {
		cfg.Storage.DSN = "./data/verifier.db"
	}
	if cfg.Storage.BackupDir == "" {
		cfg.Storage.BackupDir = "./backups"
	}
}

func validate(cfg *Config) error {
	if cfg.Discord.Token == "" {
		return fmt.Errorf("discord.token is required")
	}
	if cfg.Email.APIKey == "" {
		return fmt.Errorf("email.api_key is required")
	}
	if cfg.Email.From == "" {
		return fmt.Errorf("email.from is required")
	}
	return nil
}
