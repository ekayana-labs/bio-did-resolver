#!/usr/bin/env bash
# Cluster tests for the bio-did-registry program, driven through the
# bio-did-resolver command line. Every instruction, every guard the program
# enforces (custom error codes 6000..6014), and the resolver's use cases are
# exercised against a live cluster with throwaway keys.
#
#   NET=localnet scripts/cluster-tests.sh   # solana-test-validator with the program loaded
#   NET=devnet   scripts/cluster-tests.sh   # devnet; throwaway keys funded from FUNDER
#
# Environment:
#   NET     localnet (default) or devnet
#   RPC     RPC endpoint; defaults to the cluster's public endpoint
#   BIN     resolver binary; defaults to target/release/bio-did-resolver, then PATH
#   FUNDER  keypair that funds the throwaway keys on devnet (default: ~/.config/solana/id.json)
#   LIMITS  1 to also fill the 16 method and 16 service tables (24 extra transactions)
#   KEEP    1 to keep the throwaway keys and leave their SOL in place
#
# Needs: solana, solana-keygen, jq. Every negative case runs as --dry-run, so
# it costs nothing; the positive cases send real transactions.
set -u

NET=${NET:-localnet}
case "$NET" in
  localnet) RPC=${RPC:-http://127.0.0.1:8899}; SEG="localnet:" ;;
  devnet)   RPC=${RPC:-https://api.devnet.solana.com}; SEG="devnet:" ;;
  *) echo "NET must be localnet or devnet" >&2; exit 2 ;;
esac
HERE=$(cd "$(dirname "$0")/.." && pwd)
BIN=${BIN:-$HERE/target/release/bio-did-resolver}
[ -x "$BIN" ] || BIN=$(command -v bio-did-resolver) || { echo "bio-did-resolver not found; build it or set BIN" >&2; exit 2; }
FUNDER=${FUNDER:-$HOME/.config/solana/id.json}
LIMITS=${LIMITS:-0}
KEEP=${KEEP:-0}
KEYS=$(mktemp -d "${TMPDIR:-/tmp}/bio-did-tests.XXXXXX")

PASS=0
FAIL=0
V=0 # expected version of the subject DID

section() { printf '\n== %s\n' "$*"; }
ok() { # ok <name> <command...>: must exit 0
  local name=$1 out; shift
  if out=$("$@" 2>&1); then PASS=$((PASS + 1)); printf 'ok   %s\n' "$name"
  else FAIL=$((FAIL + 1)); printf 'FAIL %s\n%s\n' "$name" "$out"; fi
}
fails() { # fails <name> <expected text> <command...>: must fail and mention the text
  local name=$1 want=$2 out; shift 2
  if out=$("$@" 2>&1); then FAIL=$((FAIL + 1)); printf 'FAIL %s: succeeded, expected %s\n' "$name" "$want"
  elif [[ $out == *"$want"* ]]; then PASS=$((PASS + 1)); printf 'ok   %s -> %s\n' "$name" "$want"
  else FAIL=$((FAIL + 1)); printf 'FAIL %s: expected %s in\n%s\n' "$name" "$want" "$out"; fi
}
run() { "$BIN" "$@" --url "$RPC"; }
meta() { "$BIN" resolve "$1" --url "$RPC" | jq -r '.didDocumentMetadata | "\(.versionId) \(.deactivated // false)"'; }
# Writes confirm at `confirmed`; `resolve` reads at `finalized`, which lags by
# about 13 seconds. Poll until the finalized document shows the expected version.
wait_version() { # wait_version <did> <version>
  local got i
  for i in $(seq 1 45); do
    got=$(meta "$1" | cut -d' ' -f1)
    [ "$got" = "$2" ] && return 0
    sleep 1
  done
  echo "$got"; return 1
}
bump() { V=$((V + 1)); } # a write on the subject succeeded: one more version
settle() { # the finalized document must have caught up with every write so far
  local got
  if got=$(wait_version "$SUBJECT_DID" "$V"); then PASS=$((PASS + 1)); printf 'ok   subject is at version %s\n' "$V"
  else FAIL=$((FAIL + 1)); printf 'FAIL version: expected %s, resolved %s\n' "$V" "$got"; fi
}
newkey() { solana-keygen new --no-bip39-passphrase --silent --force -o "$KEYS/$1.json" >/dev/null; echo "$KEYS/$1.json"; }
pubkey() { solana address -k "$1"; }
did_of() { echo "did:bio:${SEG}$(pubkey "$1")"; }
lamports() { solana balance "$1" -u "$RPC" --lamports | cut -d' ' -f1; }
fund() { # fund <pubkey> <sol>
  if [ "$NET" = localnet ]; then solana airdrop "$2" "$1" -u "$RPC" >/dev/null
  else solana transfer "$1" "$2" -u "$RPC" --keypair "$FUNDER" --allow-unfunded-recipient >/dev/null; fi
}
repeat() { head -c "$1" /dev/zero | tr '\0' "$2"; }

# Custom program errors as they appear in transaction logs.
E_UNAUTHORIZED=0x1770
E_DEACTIVATED=0x1771
E_INVALID_FRAGMENT=0x1772
E_FRAGMENT_IN_USE=0x1773
E_VM_NOT_FOUND=0x1774
E_SERVICE_NOT_FOUND=0x1775
E_TOO_MANY_VMS=0x1776
E_TOO_MANY_SERVICES=0x1777
E_TOO_MANY_CONTROLLERS=0x1778
E_INVALID_FLAGS=0x177a
E_PROTECTED=0x177b
E_LAST_AUTHORITY=0x177c
E_INVALID_CONTROLLER=0x177d
E_INVALID_SERVICE_VALUE=0x177e

section "setup on $NET via $RPC"
SUBJECT=$(newkey subject);   SUBJECT_DID=$(did_of "$SUBJECT")
SPONSOR=$(newkey sponsor)
ROT=$(newkey rotation);      ROT_PUB=$(pubkey "$ROT")
OUTSIDER=$(newkey outsider); OUTSIDER_DID=$(did_of "$OUTSIDER")
DATASET=$(newkey dataset);   DATASET_DID=$(did_of "$DATASET"); DATASET_PUB=$(pubkey "$DATASET")
if [ "$NET" = localnet ]; then
  for k in "$SUBJECT" "$SPONSOR" "$ROT" "$OUTSIDER" "$DATASET"; do fund "$(pubkey "$k")" 5; done
else
  fund "$(pubkey "$SUBJECT")" 0.3; fund "$(pubkey "$SPONSOR")" 0.05; fund "$(pubkey "$ROT")" 0.05
  fund "$(pubkey "$OUTSIDER")" 0.02; fund "$(pubkey "$DATASET")" 0.05
fi
head -c 32 /dev/urandom > "$KEYS/x25519.bin"
head -c 33 /dev/urandom > "$KEYS/secp256k1.bin"
head -c 2592 /dev/urandom > "$KEYS/ml-dsa-87.bin"
echo "subject  $SUBJECT_DID"
echo "dataset  $DATASET_DID"

section "initialize"
ok   "generative document before init has versionId 0" test "$(meta "$SUBJECT_DID")" = "0 false"
ok   "sponsored init: sponsor pays for the subject's account" run init "$SUBJECT_DID" --keypair "$SPONSOR"
bump
ok   "self-paid init through --network (DID defaults to the keypair)" run init --keypair "$DATASET" --network "$NET"
settle
ok   "dataset DID is at version 1" wait_version "$DATASET_DID" 1
ok   "registered document keeps the generative key under all five relationships" test "$("$BIN" resolve "$SUBJECT_DID" --url "$RPC" | jq -r '[.didDocument.authentication, .didDocument.assertionMethod, .didDocument.keyAgreement, .didDocument.capabilityInvocation, .didDocument.capabilityDelegation] | map(length) | join(",")')" = "1,1,1,1,1"
fails "init twice" "uninitialized account" run init "$SUBJECT_DID" --keypair "$SPONSOR" --dry-run
fails "sponsor gained no control" "$E_UNAUTHORIZED" run add-service x T uri "$SUBJECT_DID" --keypair "$SPONSOR" --dry-run
fails "outsider cannot write" "$E_UNAUTHORIZED" run add-service x T uri "$SUBJECT_DID" --keypair "$OUTSIDER" --dry-run
fails "writes to an uninitialized DID" "Invalid account owner" run add-service x T uri "$OUTSIDER_DID" --keypair "$OUTSIDER" --dry-run
fails "cannot strip capabilityInvocation from the only authority" "$E_LAST_AUTHORITY" run set-flags default --flags authentication,protected "$SUBJECT_DID" --keypair "$SUBJECT" --dry-run
fails "cannot remove the only authority" "$E_LAST_AUTHORITY" run remove-key default "$SUBJECT_DID" --keypair "$SUBJECT" --dry-run

section "add_verification_method"
ok   "ed25519 rotation key with authentication + capabilityInvocation" run add-key rot --type ed25519 --key "$ROT_PUB" --flags authentication,capability-invocation "$SUBJECT_DID" --keypair "$SUBJECT"
bump
ok   "x25519 key with keyAgreement" run add-key kex --type x25519 --key-file "$KEYS/x25519.bin" --flags key-agreement "$SUBJECT_DID" --keypair "$SUBJECT"
bump
ok   "secp256k1 key with assertion" run add-key eth --type secp256k1 --key-file "$KEYS/secp256k1.bin" --flags assertion "$SUBJECT_DID" --keypair "$SUBJECT"
bump
# A 2592 byte ML-DSA-87 key exceeds the 1232 byte transaction limit, so the
# client uploads it through a key buffer: create, three chunks, finish.
ok   "ML-DSA-87 key (2592 bytes) uploaded through a key buffer" run add-key pq --type ml-dsa-87 --key-file "$KEYS/ml-dsa-87.bin" --flags assertion "$SUBJECT_DID" --keypair "$SUBJECT"
bump
fails "no key buffer is left behind" "Invalid account owner" run close-key-buffer "$SUBJECT_DID" --keypair "$SUBJECT" --dry-run
ok   "the rotation key is now an authority: it signs a write" run add-service tmp Tmp https://example.org/tmp "$SUBJECT_DID" --keypair "$ROT"
bump
ok   "born protected under its own key" run add-key rotp --type ed25519 --key "$ROT_PUB" --flags capability-invocation,protected "$SUBJECT_DID" --keypair "$ROT"
bump
settle
ok   "document lists six verification methods" test "$("$BIN" resolve "$SUBJECT_DID" --url "$RPC" | jq '.didDocument.verificationMethod | length')" = 6
ok   "the post quantum key materializes as a JsonWebKey under assertionMethod" test "$("$BIN" resolve "$SUBJECT_DID" --url "$RPC" | jq -r '(.didDocument.verificationMethod[] | select(.id | endswith("#pq")) | .type) + "," + ([.didDocument.assertionMethod[] | select(endswith("#pq"))] | length | tostring)')" = "JsonWebKey,1"
ok   "x25519 key appears only under keyAgreement" test "$("$BIN" resolve "$SUBJECT_DID" --url "$RPC" | jq -r '[.didDocument.keyAgreement[] | select(endswith("#kex"))] | length')" = 1
fails "x25519 cannot authenticate" "$E_INVALID_FLAGS" run add-key bad --type x25519 --key-file "$KEYS/x25519.bin" --flags authentication "$SUBJECT_DID" --keypair "$SUBJECT" --dry-run
fails "only ed25519 may hold capabilityInvocation" "$E_INVALID_FLAGS" run add-key bad --type secp256k1 --key-file "$KEYS/secp256k1.bin" --flags capability-invocation "$SUBJECT_DID" --keypair "$SUBJECT" --dry-run
fails "duplicate fragment" "$E_FRAGMENT_IN_USE" run add-key default --type ed25519 --key "$ROT_PUB" --flags authentication "$SUBJECT_DID" --keypair "$SUBJECT" --dry-run
fails "fragment with invalid characters" "$E_INVALID_FRAGMENT" run add-key "bad fragment!" --type ed25519 --key "$ROT_PUB" --flags authentication "$SUBJECT_DID" --keypair "$SUBJECT" --dry-run
fails "fragment longer than 32" "$E_INVALID_FRAGMENT" run add-key "$(repeat 33 a)" --type ed25519 --key "$ROT_PUB" --flags authentication "$SUBJECT_DID" --keypair "$SUBJECT" --dry-run
fails "cannot plant a protected key that is not the signer's" "$E_PROTECTED" run add-key plant --type ed25519 --key "$(pubkey "$OUTSIDER")" --flags capability-invocation,protected "$SUBJECT_DID" --keypair "$SUBJECT" --dry-run
fails "client rejects wrong key length before sending" "keys are 32 bytes" run add-key bad --type ed25519 --key 3xyz --flags authentication "$SUBJECT_DID" --keypair "$SUBJECT"
fails "client rejects unknown relationship names" "unknown relationship" run add-key bad --type ed25519 --key "$ROT_PUB" --flags admin "$SUBJECT_DID" --keypair "$SUBJECT"

section "set_verification_method_flags"
ok   "authority changes an unprotected key's flags" run set-flags rot --flags authentication "$SUBJECT_DID" --keypair "$SUBJECT"
bump
ok   "a key's own holder changes its protected flags" run set-flags default --flags authentication,assertion,capability-invocation,protected "$SUBJECT_DID" --keypair "$SUBJECT"
bump
fails "protected key cannot be changed by another authority" "$E_PROTECTED" run set-flags default --flags authentication "$SUBJECT_DID" --keypair "$ROT" --dry-run
fails "granting protection needs the key's own holder" "$E_PROTECTED" run set-flags rot --flags authentication,protected "$SUBJECT_DID" --keypair "$SUBJECT" --dry-run
ok   "the key's holder grants itself protection" run set-flags rot --flags authentication,capability-invocation,protected "$SUBJECT_DID" --keypair "$ROT"
bump
settle
ok   "rot now appears under capabilityInvocation" test "$("$BIN" resolve "$SUBJECT_DID" --url "$RPC" | jq -r '[.didDocument.capabilityInvocation[] | select(endswith("#rot"))] | length')" = 1
fails "flags not permitted for the key type" "$E_INVALID_FLAGS" run set-flags kex --flags authentication "$SUBJECT_DID" --keypair "$SUBJECT" --dry-run
fails "unknown fragment" "$E_VM_NOT_FOUND" run set-flags nope --flags authentication "$SUBJECT_DID" --keypair "$SUBJECT" --dry-run

section "remove_verification_method"
fails "unknown fragment" "$E_VM_NOT_FOUND" run remove-key nope "$SUBJECT_DID" --keypair "$SUBJECT" --dry-run
fails "protected key cannot be removed by another authority" "$E_PROTECTED" run remove-key rot "$SUBJECT_DID" --keypair "$SUBJECT" --dry-run
fails "protected default key cannot be removed by the rotation key" "$E_PROTECTED" run remove-key default "$SUBJECT_DID" --keypair "$ROT" --dry-run
ok   "the holder drops its own protection" run set-flags rot --flags authentication "$SUBJECT_DID" --keypair "$ROT"
bump
ok   "authority removes the unprotected key" run remove-key rot "$SUBJECT_DID" --keypair "$SUBJECT"
bump
ok   "holder removes its own protected authority key (another authority remains)" run remove-key rotp "$SUBJECT_DID" --keypair "$ROT"
bump
fails "the removed key no longer signs" "$E_UNAUTHORIZED" run add-service x T uri "$SUBJECT_DID" --keypair "$ROT" --dry-run
ok   "remove the x25519 key" run remove-key kex "$SUBJECT_DID" --keypair "$SUBJECT"
bump
settle
ok   "three verification methods remain" test "$("$BIN" resolve "$SUBJECT_DID" --url "$RPC" | jq '.didDocument.verificationMethod | length')" = 3

section "add_service / remove_service"
ok   "BioMetadata service on IPFS" run add-service metadata BioMetadata ipfs://bafybeigdyrzt5sfp7udm7hu76uh7y26nf3efuylqabf3oclgtqy55fbzdi "$SUBJECT_DID" --keypair "$SUBJECT"
bump
ok   "DataverseRepository service" run add-service repo DataverseRepository https://doi.org/10.5072/FK2/EXAMPLE "$SUBJECT_DID" --keypair "$SUBJECT"
bump
fails "duplicate service fragment" "$E_FRAGMENT_IN_USE" run add-service metadata BioMetadata ipfs://x "$SUBJECT_DID" --keypair "$SUBJECT" --dry-run
fails "fragments are shared with verification methods" "$E_FRAGMENT_IN_USE" run add-service default BioMetadata ipfs://x "$SUBJECT_DID" --keypair "$SUBJECT" --dry-run
fails "endpoint with whitespace" "$E_INVALID_SERVICE_VALUE" run add-service bad BioMetadata "ipfs://bad cid" "$SUBJECT_DID" --keypair "$SUBJECT" --dry-run
fails "empty service type" "$E_INVALID_SERVICE_VALUE" run add-service bad "" ipfs://x "$SUBJECT_DID" --keypair "$SUBJECT" --dry-run
fails "service type longer than 64" "$E_INVALID_SERVICE_VALUE" run add-service bad "$(repeat 65 T)" ipfs://x "$SUBJECT_DID" --keypair "$SUBJECT" --dry-run
fails "endpoint longer than 512" "$E_INVALID_SERVICE_VALUE" run add-service bad BioMetadata "https://x/$(repeat 505 a)" "$SUBJECT_DID" --keypair "$SUBJECT" --dry-run
fails "unknown service" "$E_SERVICE_NOT_FOUND" run remove-service nope "$SUBJECT_DID" --keypair "$SUBJECT" --dry-run
ok   "remove a service" run remove-service tmp "$SUBJECT_DID" --keypair "$SUBJECT"
bump
settle
ok   "two services remain, with their types" test "$("$BIN" resolve "$SUBJECT_DID" --url "$RPC" | jq -r '[.didDocument.service[].type] | sort | join(",")')" = "BioMetadata,DataverseRepository"

section "set_controllers"
ok   "dataset controlled by a did:bio key and a did:web institution" run set-controllers --controller "$DATASET_PUB" --external did:web:lab.example.org "$SUBJECT_DID" --keypair "$SUBJECT"
bump
settle
ok   "controllers appear in the document" test "$("$BIN" resolve "$SUBJECT_DID" --url "$RPC" | jq -r '.didDocument.controller | length')" = 2
fails "self reference" "$E_INVALID_CONTROLLER" run set-controllers --controller "$(pubkey "$SUBJECT")" "$SUBJECT_DID" --keypair "$SUBJECT" --dry-run
fails "duplicate native controller" "$E_INVALID_CONTROLLER" run set-controllers --controller "$DATASET_PUB" --controller "$DATASET_PUB" "$SUBJECT_DID" --keypair "$SUBJECT" --dry-run
fails "duplicate external controller" "$E_INVALID_CONTROLLER" run set-controllers --external did:web:a.example --external did:web:a.example "$SUBJECT_DID" --keypair "$SUBJECT" --dry-run
fails "did:bio must use the native form" "$E_INVALID_CONTROLLER" run set-controllers --external "$DATASET_DID" "$SUBJECT_DID" --keypair "$SUBJECT" --dry-run
fails "external controller must be a DID" "$E_INVALID_CONTROLLER" run set-controllers --external https://lab.example.org "$SUBJECT_DID" --keypair "$SUBJECT" --dry-run
fails "external controller with whitespace" "$E_INVALID_CONTROLLER" run set-controllers --external "did:web:bad host" "$SUBJECT_DID" --keypair "$SUBJECT" --dry-run
fails "external controller longer than 128" "$E_INVALID_CONTROLLER" run set-controllers --external "did:web:$(repeat 121 a)" "$SUBJECT_DID" --keypair "$SUBJECT" --dry-run
many_native=(); many_external=()
for i in 1 2 3 4 5 6 7 8 9; do
  many_native+=(--controller "$(pubkey "$(newkey "ctrl-$i")")"); many_external+=(--external "did:web:$i.example.org")
done
fails "more than 8 native controllers" "$E_TOO_MANY_CONTROLLERS" run set-controllers "${many_native[@]}" "$SUBJECT_DID" --keypair "$SUBJECT" --dry-run
fails "more than 8 external controllers" "$E_TOO_MANY_CONTROLLERS" run set-controllers "${many_external[@]}" "$SUBJECT_DID" --keypair "$SUBJECT" --dry-run
ok   "eight of each is the maximum" run set-controllers "${many_native[@]:0:16}" "${many_external[@]:0:16}" "$SUBJECT_DID" --keypair "$SUBJECT" --dry-run
ok   "clear all controllers" run set-controllers "$SUBJECT_DID" --keypair "$SUBJECT"
bump
settle
ok   "controller field is gone" test "$("$BIN" resolve "$SUBJECT_DID" --url "$RPC" | jq '.didDocument | has("controller")')" = false

if [ "$LIMITS" = 1 ]; then
  section "table limits (LIMITS=1)"
  count() { "$BIN" resolve "$SUBJECT_DID" --url "$RPC" | jq ".didDocument.$1 // [] | length"; }
  n=$(count verificationMethod); i=0
  while [ "$n" -lt 16 ]; do
    i=$((i + 1)); n=$((n + 1)); head -c 32 /dev/urandom > "$KEYS/x-$i.bin"
    ok "verification method $n of 16" run add-key "x-$i" --type x25519 --key-file "$KEYS/x-$i.bin" --flags key-agreement "$SUBJECT_DID" --keypair "$SUBJECT"
    bump
  done
  fails "17th verification method" "$E_TOO_MANY_VMS" run add-key x-17 --type x25519 --key-file "$KEYS/x-1.bin" --flags key-agreement "$SUBJECT_DID" --keypair "$SUBJECT" --dry-run
  n=$(count service); i=0
  while [ "$n" -lt 16 ]; do
    i=$((i + 1)); n=$((n + 1))
    ok "service $n of 16" run add-service "svc-$i" Test "https://example.org/$i" "$SUBJECT_DID" --keypair "$SUBJECT"
    bump
  done
  fails "17th service" "$E_TOO_MANY_SERVICES" run add-service svc-17 Test https://example.org/17 "$SUBJECT_DID" --keypair "$SUBJECT" --dry-run
  settle
  ok   "both tables are full" test "$(count verificationMethod),$(count service)" = "16,16"
fi

section "deactivate"
fails "refused without --yes" "pass --yes" run deactivate "$SUBJECT_DID" --keypair "$SUBJECT"
fails "outsider cannot deactivate" "$E_UNAUTHORIZED" run deactivate "$SUBJECT_DID" --keypair "$OUTSIDER" --yes --dry-run
before=$(lamports "$(pubkey "$SUBJECT")")
ok   "authority deactivates with --yes" run deactivate "$SUBJECT_DID" --keypair "$SUBJECT" --yes
bump
settle
after=$(lamports "$(pubkey "$SUBJECT")")
ok   "rent above the tombstone came back to the payer ($before -> $after lamports)" test "$after" -gt "$before"
ok   "resolves as deactivated" test "$(meta "$SUBJECT_DID")" = "$V true"
ok   "deactivated document keeps only the id" test "$("$BIN" resolve "$SUBJECT_DID" --url "$RPC" | jq -r '.didDocument | keys | join(",")')" = "@context,id"
fails "no writes after deactivation" "$E_DEACTIVATED" run add-service x T uri "$SUBJECT_DID" --keypair "$SUBJECT" --dry-run
fails "no second deactivation" "$E_DEACTIVATED" run deactivate "$SUBJECT_DID" --keypair "$SUBJECT" --yes --dry-run
fails "tombstone never resurrects through init" "uninitialized account" run init "$SUBJECT_DID" --keypair "$SPONSOR" --dry-run

section "resolver use cases"
ok   "generative DID: any keypair resolves at zero cost" test "$(meta "$OUTSIDER_DID")" = "0 false"
ok   "generative document carries the key under all five relationships" test "$("$BIN" resolve "$OUTSIDER_DID" --url "$RPC" | jq -r '[.didDocument.authentication, .didDocument.assertionMethod, .didDocument.keyAgreement, .didDocument.capabilityInvocation, .didDocument.capabilityDelegation] | map(length) | join(",")')" = "1,1,1,1,1"
ok   "registered DID reports versionId and updated" test "$("$BIN" resolve "$DATASET_DID" --url "$RPC" | jq -r '.didDocumentMetadata | has("versionId") and has("updated")')" = true
ok   "content type is application/did+ld+json" test "$("$BIN" resolve "$DATASET_DID" --url "$RPC" | jq -r '.didResolutionMetadata.contentType')" = "application/did+ld+json"
fails "malformed DID" "invalid DID" "$BIN" resolve did:bio:devnet:notakey
fails "DID URL fragments are not resolvable here" "invalid DID" "$BIN" resolve "$DATASET_DID#default"
fails "RPC failure is an error, never the generative fallback" "fetching the registry account" "$BIN" resolve "$DATASET_DID" --url http://127.0.0.1:1
fails "mainnet DIDs carry no network segment and need --yes" "refusing to send" run add-service x T uri "did:bio:$(pubkey "$DATASET")" --keypair "$DATASET"
ok   "--dry-run reports compute units without sending" bash -c "$(printf '%q ' "$BIN" add-service x T https://example.org "$DATASET_DID" --keypair "$DATASET" --url "$RPC" --dry-run) | grep -q 'compute units'"
ok   "dataset DID unchanged by the dry run" test "$(meta "$DATASET_DID")" = "1 false"
ok   "sponsor a stranger's DID from the platform key" run init "$OUTSIDER_DID" --keypair "$SPONSOR"
ok   "the stranger, not the sponsor, controls it" run add-service metadata BioMetadata ipfs://bafybeigdyrzt5sfp7udm7hu76uh7y26nf3efuylqabf3oclgtqy55fbzdi "$OUTSIDER_DID" --keypair "$OUTSIDER"
ok   "the stranger's document reaches version 2" wait_version "$OUTSIDER_DID" 2
ok   "service endpoint is readable through resolve" test "$("$BIN" resolve "$OUTSIDER_DID" --url "$RPC" | jq -r '.didDocument.service[0].serviceEndpoint')" = "ipfs://bafybeigdyrzt5sfp7udm7hu76uh7y26nf3efuylqabf3oclgtqy55fbzdi"

section "summary"
if [ "$NET" = devnet ] && [ "$KEEP" != 1 ]; then
  funder_pub=$(pubkey "$FUNDER")
  for k in "$SUBJECT" "$SPONSOR" "$ROT" "$OUTSIDER" "$DATASET"; do
    solana transfer "$funder_pub" ALL --keypair "$k" -u "$RPC" >/dev/null 2>&1 || true
  done
  echo "leftover SOL returned to $funder_pub"
fi
[ "$KEEP" = 1 ] && echo "keys kept in $KEYS" || rm -rf "$KEYS"
printf '%d passed, %d failed\n' "$PASS" "$FAIL"
[ "$FAIL" = 0 ]
