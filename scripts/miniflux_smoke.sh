#!/usr/bin/env bash
set -euo pipefail

BASE=${BASE:-http://localhost:8080}
USER=${USER_NAME:-alice}
PASS=${USER_PASS:-secret}

have_jq=1
command -v jq >/dev/null 2>&1 || have_jq=0

log() { printf "\033[1;34m==> %s\033[0m\n" "$*"; }
say() { printf "%s\n" "$*"; }

json_get() {
  if [ $have_jq -eq 1 ]; then jq -r "$2"; else cat; fi
}

log "BASE=$BASE"

log "Create user if empty"
set +e
curl -sS -X POST "$BASE/api/v1/users" -H 'content-type: application/json' \
  -d '{"username":"'$USER'","password":"'$PASS'"}' >/dev/null
set -e

log "Login"
TOKEN=$(curl -sS -X POST "$BASE/api/v1/auth/login" -H 'content-type: application/json' \
  -d '{"username":"'$USER'","password":"'$PASS'"}' | json_get .token)
if [ -z "$TOKEN" ] || [ "$TOKEN" = "null" ]; then
  echo "Login failed" >&2; exit 1
fi
say "TOKEN=$TOKEN"

hdr=(-H "X-Auth-Token: $TOKEN" -H 'content-type: application/json')

log "Create category"
curl -sS -X POST "$BASE/v1/categories" "${hdr[@]}" -d '{"title":"tech"}' | ( [ $have_jq -eq 1 ] && jq . || cat )

log "Create feed"
feed_resp=$(curl -sS -X POST "$BASE/v1/feeds" "${hdr[@]}" -d '{"url":"https://blog.rust-lang.org/feed.xml"}')
say "$feed_resp"
if [ $have_jq -eq 1 ]; then FEED_ID=$(printf "%s" "$feed_resp" | jq -r .id); else FEED_ID=1; fi
say "FEED_ID=$FEED_ID"

log "Refresh feed"
curl -sS -X POST "$BASE/v1/feeds/$FEED_ID/refresh" -H "X-Auth-Token: $TOKEN" | ( [ $have_jq -eq 1 ] && jq . || cat )

log "List entries"
entries=$(curl -sS "$BASE/v1/entries?limit=3" -H "X-Auth-Token: $TOKEN")
say "$entries"
if [ $have_jq -eq 1 ]; then ENTRY_ID=$(printf "%s" "$entries" | jq -r .entries[0].id); else ENTRY_ID=1; fi
say "ENTRY_ID=$ENTRY_ID"

log "Tag entry"
curl -sS -X POST "$BASE/v1/entries/$ENTRY_ID/tags" "${hdr[@]}" -d '{"tags":["test","rust"]}' || true
curl -sS "$BASE/v1/tags" -H "X-Auth-Token: $TOKEN" | ( [ $have_jq -eq 1 ] && jq . || cat )

log "Discover"
curl -sS -X POST "$BASE/v1/discover" "${hdr[@]}" -d '{"url":"https://blog.rust-lang.org/"}' | ( [ $have_jq -eq 1 ] && jq . || cat )

log "Counters & Version"
curl -sS "$BASE/v1/feeds/counters" -H "X-Auth-Token: $TOKEN" | ( [ $have_jq -eq 1 ] && jq . || cat )
curl -sS "$BASE/v1/version" -H "X-Auth-Token: $TOKEN" | ( [ $have_jq -eq 1 ] && jq . || cat )

log "Done"

