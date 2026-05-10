# Deployment artifacts

Drop-in configurations for running `cerridwen-server` in production.

## Docker

```bash
# Build the image (multi-stage, ~150 MB final).
docker build -t cerridwen .

# Multi-arch build (amd64 + arm64) using buildx:
docker buildx create --use --name cerridwen-builder
docker buildx build \
  --platform linux/amd64,linux/arm64 \
  --tag your-registry/cerridwen:latest \
  --push \
  .

# Run with the default port.
docker run -p 2828:2828 --name cerridwen cerridwen

# With persistent events DB:
docker run -p 2828:2828 \
  -v cerridwen-data:/var/lib/cerridwen \
  -e CERRIDWEN_EVENTS_DB=/var/lib/cerridwen/events.db \
  cerridwen
```

Health check is built into the image (`/health`). The container runs as a
non-root `cerridwen` user.

## systemd

1. Build the binary: `cargo build --release --features server,mcp,events`.
2. Create user + dirs:
   ```bash
   sudo useradd -r -s /usr/sbin/nologin cerridwen
   sudo mkdir -p /opt/cerridwen /var/lib/cerridwen
   sudo cp target/release/cerridwen-server /opt/cerridwen/
   sudo cp -r ../sweph /opt/cerridwen/
   sudo chown -R cerridwen:cerridwen /opt/cerridwen /var/lib/cerridwen
   ```
3. Install the unit:
   ```bash
   sudo cp deploy/cerridwen.service /etc/systemd/system/
   sudo systemctl daemon-reload
   sudo systemctl enable --now cerridwen
   ```

The unit applies sandboxing (`NoNewPrivileges`, `ProtectSystem=strict`,
`PrivateTmp`, syscall filter, etc.) — adjust if you need to write outside
`/var/lib/cerridwen`.

## nginx (TLS termination + reverse proxy)

`deploy/nginx-cerridwen.conf` shows a typical layout:

- HTTP → HTTPS redirect
- TLS 1.2/1.3 with Let's Encrypt certs
- Special-cased SSE streams (`proxy_buffering off`, long read timeout)
- Optional caching for `/openapi.json` and `/favicon.ico`
- Tight `/metrics` ACL (commented out — uncomment for internal-only scrape)

```bash
sudo cp deploy/nginx-cerridwen.conf /etc/nginx/sites-available/cerridwen
sudo ln -s /etc/nginx/sites-available/cerridwen /etc/nginx/sites-enabled/
sudo certbot --nginx -d cerridwen.example.com    # provisions the certs
sudo nginx -t && sudo systemctl reload nginx
```

## Observability

- `/health` — liveness probe (returns `{ "status": "ok", … }`)
- `/metrics` — Prometheus exposition format

Hooked into Prometheus via:

```yaml
scrape_configs:
  - job_name: cerridwen
    metrics_path: /metrics
    static_configs:
      - targets: ['cerridwen.example.com:443']
        labels: { service: cerridwen }
    scheme: https
```

## Environment variables

| name                       | meaning                                          |
| -------------------------- | ------------------------------------------------ |
| `CERRIDWEN_EPHE_PATH`      | Swiss Ephemeris data directory (`./sweph` default) |
| `CERRIDWEN_EVENTS_DB`      | sqlite events DB path (`./events.db` default)    |
| `RUST_LOG`                 | tracing filter, e.g. `info,cerridwen=debug`      |
