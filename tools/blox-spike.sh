#!/usr/bin/env bash
# Spike 1 for the publisher subscriptions design (§11): what a BLOX (TownNews) site hands a
# signed-in subscriber. Run it yourself with your own account; it prints shapes, never
# secrets (the auth token and the password are redacted or never shown).
#
#   tools/blox-spike.sh gazette.example you@example.com 'https://www.gazette.example/news/.../article_….html'
#
# It asks for the password without echo, signs in through the site's own login form, then
# reports: the cookie names the site set, the decoded `tncms-user` cookie with its token
# redacted, whether a premium page comes back already decrypted for a subscriber (or still
# ROT47 `encrypted-content`), and what `/tncms/subscription/services/` returns. Paste the
# output into the PR; nothing in it identifies the account beyond what you choose to keep.
set -euo pipefail
site=${1:?site, e.g. gazette.example}
user=${2:?username (email)}
page=${3:?a premium article URL on the site}
ua='Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/605.1.15 (KHTML, like Gecko) Version/18.0 Safari/605.1.15'
jar=$(mktemp -t blox-spike-jar)
trap 'rm -f "$jar"' EXIT
read -r -s -p "Password for $user at $site: " pw; echo

login=$(curl -sL -A "$ua" -c "$jar" -b "$jar" -w '%{url_effective}' -o /tmp/blox-spike-login.html "https://$site/users/login/")
action=$(grep -o '<form id="access-modal-login-form"[^>]*action="[^"]*"' /tmp/blox-spike-login.html | sed 's/.*action="//; s/"$//')
hidden=$(grep -o '<input[^>]*type="hidden"[^>]*>' /tmp/blox-spike-login.html | grep -o 'name="[^"]*" value="[^"]*"' | sed 's/name="//; s/" value="/=/; s/"$//' | paste -sd '&' -)
echo "login page final URL: $login"
echo "form action:          ${action:-<not found>}"
echo "hidden fields:        ${hidden//=*/=…}"
code=$(curl -s -A "$ua" -c "$jar" -b "$jar" -o /tmp/blox-spike-post.html -w '%{http_code} %{redirect_url}' \
  --data-urlencode "username=$user" --data-urlencode "password=$pw" ${hidden:+--data "$hidden"} "${action:-https://www.$site/users/login/}")
unset pw
echo "login POST:           $code"
echo
echo "cookies set (name, domain, path, expiry):"
awk '!/^#/ && NF >= 7 { printf "  %-28s %-24s %-6s %s\n", $6, $1, $3, ($5 == 0 ? "session" : "expires") }' "$jar"
echo
echo "tncms-user cookie, URL-decoded, token redacted:"
awk '!/^#/ && $6 == "tncms-user" { print $7 }' "$jar" | python3 -c 'import sys,urllib.parse,re; v=urllib.parse.unquote(sys.stdin.read().strip()); print("  " + re.sub(r"([A-Za-z0-9_-]{24,})", "<redacted>", v) if v else "  <no tncms-user cookie>")'
echo
curl -sL -A "$ua" -b "$jar" -o /tmp/blox-spike-page.html "$page"
enc=$(grep -c 'subscriber-only encrypted-content' /tmp/blox-spike-page.html || true)
ce=$(grep -o 'data-content-encrypted="[^"]*"' /tmp/blox-spike-page.html | head -1)
sn=$(grep -o '<span class="tn-user-screenname">[^<]*' /tmp/blox-spike-page.html | head -1 | sed 's/.*>//')
echo "premium page as a subscriber:"
echo "  encrypted-content blocks: $enc   ${ce:-<no data-content-encrypted attr>}"
echo "  tn-user-screenname text:  '${sn}'"
echo "  bloxServiceIDs / data-subscription-service:"
grep -o 'bloxServiceIDs = [^;]*\|data-subscription-service="[^"]*"\|data-service-id="[^"]*"' /tmp/blox-spike-page.html | sort -u | sed 's/^/    /'
echo
echo "/tncms/subscription/services/ (first 600 bytes):"
curl -sL -A "$ua" -b "$jar" "https://www.$site/tncms/subscription/services/" | head -c 600; echo
rm -f /tmp/blox-spike-login.html /tmp/blox-spike-post.html /tmp/blox-spike-page.html
