FROM golang:1.27-alpine AS builder

WORKDIR /app

# Dependencies
COPY go.mod go.sum ./
RUN go mod download

# Source code
COPY . .

# Build static binary
# CGO_ENABLED=0 because modernc.org sqlite is pure-go, we don't need CGO!
RUN CGO_ENABLED=0 GOOS=linux go build -o /app/verifier-bot ./cmd/bot

FROM alpine:latest
RUN apk --no-cache add ca-certificates tzdata

WORKDIR /app
COPY --from=builder /app/verifier-bot .
COPY deploy/config.yml .

# Database folder
RUN mkdir -p /app/data

# Run the bot
CMD ["./verifier-bot"]

