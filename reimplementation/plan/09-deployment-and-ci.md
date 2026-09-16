# Session 9 — Deployment, CI & Final Polish

> **Goal:** containerize the bot, add CI, provide env/config examples, run a full test pass, and
> produce a release binary. This closes the loop on parity with the Go project's deployment story
> (`deploy/`).
>
> **Parity target:** Go `deploy/` (`.example.env`, `config.yml`, `docker/Dockerfile`,
> `docker/docker-compose.yml`) and `.github/workflows/go.yml`.

---

## 9.1 Example env & config (repo root)

`.env.example` — parity with Go `deploy/.example.env`:

```dotenv
DISCORD_TOKEN=your-bot-token-here
RESEND_API_KEY=re_your_resend_key
```

`config.example.yml` — parity with Go `deploy/config.yml`:

```yaml
discord:
  token: ${DISCORD_TOKEN}

email:
  api_key: ${RESEND_API_KEY}
  from: "Discord bot <discord-bot@yourdomain.com>"

storage:
  dsn: "./data/verifier.db"
  backup_dir: "./backups"
```

`.gitignore` additions: `config.yml`, `data/`, `backups/`, `*.db`, `.env`, `/target`.

## 9.2 Dockerfile

Go's build was multi-stage and CGO-free; ours is the same with `rust` build image.

```dockerfile
# ---- Builder ----
FROM rust:1-bookworm AS builder
WORKDIR /build
COPY Cargo.toml Cargo.lock ./
COPY src ./src
RUN cargo build --release --locked

# ---- Runtime ----
FROM debian:bookworm-slim
RUN apt-get update && apt-get install -y --no-install-recommends ca-certificates tzdata \
    && rm -rf /var/lib/apt/lists/*
COPY --from=builder /build/target/release/multipurpose-discord-bot /usr/local/bin/
ENV TZ=UTC
WORKDIR /app
ENTRYPOINT ["multipurpose-discord-bot", "--config", "config.yml"]
```

> `rustls` + `rusqlite/bundled` produce a fully static binary (no glibc openssl/libsqlite), so
> `distroless/static` is an even smaller alternative runtime image. Choose based on preference;
> `debian-slim` keeps debugging easy.

## 9.3 docker-compose.yml

Parity with Go compose (single service, SQLite volume, read-only config, env from `.env`):

```yaml
services:
  discord-bot:
    build: .
    image: multipurpose-discord-bot:latest
    restart: unless-stopped
    volumes:
      - ./data:/app/data           # SQLite + backups persistence
      - ./config.yml:/app/config.yml:ro
    env_file:
      - .env
```

## 9.4 CI — `.github/workflows/ci.yml`

Parity with Go `go.yml` (build + test on push/PR to main), adapted to Rust tooling:

```yaml
name: CI
on:
  push: { branches: [main] }
  pull_request:

jobs:
  build-test:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - uses: dtolnay/rust-toolchain@stable
        with:
          components: rustfmt, clippy
      - uses: Swatinem/rust-cache@v2
      - name: Format
        run: cargo fmt --check
      - name: Clippy
        run: cargo clippy -- -D warnings
      - name: Test
        run: cargo test --all-targets
      - name: Build release
        run: cargo build --release --locked
```

Add a **database-parity smoke test** to CI: check in a tiny fixture SQLite DB created by the Go
bot (or a `.sql` dump), open it with the Rust store in a test, and assert the schema matches
(`PRAGMA table_info` for all 10 tables + indexes). This makes the "works with old databases"
guarantee a CI-enforced invariant.

## 9.5 Final verification & parity sweep

Full checklist against the Go reference before declaring done:

- [ ] **Command surface** — every slash command/subcommand/option from `reimplementation/08-discord-commands-reference.md` exists with identical names, types, required/optional.
- [ ] **Interaction ids** — `btn_verify_start`, `btn_enter_code`, `modal_email`, `modal_code`, `input_email`, `input_code` exact.
- [ ] **DB schema** — 10 tables, 1 index, same DDL; legacy migrations idempotent; old Go DB opens and operates.
- [ ] **Verification semantics** — error-to-locale mapping identical; code TTL/max attempts/rate-limit defaults identical.
- [ ] **Backup JSON** — round-trips a real Go-generated backup.
- [ ] **i18n** — every string matches Go translations; both locales complete.
- [ ] **Config** — `config.yml` from the Go repo loads unchanged.
- [ ] **Flags** — `--config` / `--debug` work.
- [ ] **README** — quick-start mirrors Go README (env vars, intents requirement, permissions).

## 9.6 Release

```bash
cargo build --release
./target/release/multipurpose-discord-bot --config config.yml
```

Resource expectations (parity): ~10–20 MB RSS idle (Go bot targeted < 50 MB), single process,
no listening ports, WAL SQLite on disk.

---

## 9.7 Session completion checklist

- [ ] `.env.example`, `config.example.yml`, `.gitignore` committed
- [ ] `Dockerfile` + `docker-compose.yml` build & run cleanly
- [ ] CI passes (fmt, clippy, tests, release build, DB-parity smoke test)
- [ ] Full parity sweep from §9.5
- [ ] `cargo test` green, release binary produced
- [ ] README updated with intents + permission requirements
