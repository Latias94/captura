# Authentication

## Local username/password

- Endpoint: `POST /api/v1/auth/login` → `{ "token": "..." }`
- Disable local login: `CAPTURA_DISABLE_LOCAL_AUTH=1` (also disables Basic password on compatibility endpoints)

## API Keys (Miniflux-compatible)

- Endpoints under `/v1/api-keys` for list/create/delete.
- Use header `X-Auth-Token` to access protected endpoints.

## Reverse Proxy Auth

- Configure your reverse-proxy to inject a trusted header (e.g., `X-Forwarded-User`).
- Server config:
  - `CAPTURA_AUTH_PROXY_HEADER=X-Forwarded-User`
  - Optional: `CAPTURA_AUTH_PROXY_USER_CREATION=1` to auto-create users.
- Token minting helper: `GET /api/v1/auth/proxy/token` (reads the trusted header and returns an API token).

## OIDC / Google (Authorization Code)

- Enable and configure:
  - `CAPTURA_OIDC_ENABLED=1`
  - `CAPTURA_OIDC_ISSUER_URL=https://accounts.google.com`
  - `CAPTURA_OIDC_CLIENT_ID=...`
  - `CAPTURA_OIDC_CLIENT_SECRET=...`
  - `CAPTURA_OIDC_REDIRECT_URL=http://localhost:8080/api/v1/auth/oidc/callback`
  - Optional: `CAPTURA_OIDC_STATE_SECRET=`
- Flow:
  - `GET /api/v1/auth/oidc/start` → redirects to provider
  - `GET /api/v1/auth/oidc/callback?code=...&state=...` → returns `{ "token": "..." }`
  - HTML view: send `Accept: text/html` or header `x-view: html`.

## Notes

- Use `X-Auth-Token` for API access.
- For clients using Miniflux/Fever/Google Reader compatibility, Basic auth with password is supported unless `CAPTURA_DISABLE_LOCAL_AUTH=1`.

