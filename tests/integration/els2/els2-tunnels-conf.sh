#!/usr/bin/env bash
# Plan 381 §WP1 — the i2pd ELS2 `tunnels.conf` writer and validator.
#
# This file is a *library*, not a lane. It holds two named functions and has no
# side effects beyond defining them, so both the cheap contract test
# (`test-tunnels-conf.sh`) and the expensive lane runner
# (`run-i2pd-els2.sh`) source the same code.
#
# This deliberately departs from the Plan 215 convention
# (`tests/integration/service-tunnels/test-plan215-tunnels-conf.sh`), which
# duplicates the writer inline "so the contract can be re-verified
# independently of the expensive external lane". The stated reason was
# independence from the external lane, and a sourced shell library already has
# that: it runs no i2pd, touches no network, and needs no binary. What the
# duplication actually bought was two copies that could disagree, with the
# divergence comment admitting it. Sharing one copy removes that failure mode
# instead of documenting it. See `plans/closure/i2pcontrol-proposal-170/381-status.md`.
#
# ---------------------------------------------------------------------------
# The key spelling below is not a preference; it is forced by the reference.
# Every rule is source-cited or probe-verified at the i2pd pin
# 635b013a612ff47278ef02acf8580a28e10e26c5 (2.61.0), and the citations are
# reproduced in `tests/integration/els2/reference-freeze.md` §3.3:
#
#   * The auth modes are a closed set, NONE=0, DH=1, PSK=2
#     (`libi2pd/LeaseSet.h:290-292`).
#   * `i2cp.leaseSetType = 5` selects the encrypted LeaseSet2
#     (`libi2pd/Destination.cpp:75-77`).
#   * The group is selected *by auth type*
#     (`libi2pd/Destination.cpp:1088-1091`), so the mode and the key group must
#     be paired: DH reads `i2cp.leaseSetClient.dh`, PSK reads
#     `i2cp.leaseSetClient.psk`. Pairing a PSK key with `= 1` silently reads the
#     wrong group.
#   * The reader is a *prefix* match, so the bare and the indexed spelling are
#     both accepted (`libi2pd_client/ClientContext.cpp:465-473`,
#     `libi2pd/Destination.cpp:1607-1622`). This lane emits the indexed form
#     because it is the shape i2pd's own source comment documents.
#   * **The value must contain a colon.** `ReadAuthKey` keeps only the bytes
#     after the first `:` and drops a colon-less entry *with no diagnostic*
#     (`libi2pd/Destination.cpp:1612-1620`). A bare base64 value is accepted by
#     the config parser, discarded, and the destination then publishes with an
#     empty auth-key set — an authentication failure that looks exactly like a
#     crypto defect. Probe-verified: with `= <base64>` i2pd logs
#     `No auth keys read for auth type: 2`; with `= 0:<base64>` it logs
#     `1 auth keys read`.
#   * The key is exactly 32 bytes
#     (`i2p::data::Tag<32> AuthPublicKey`, `libi2pd/LeaseSet.h:294`).
#
# Nothing here is a workaround. It is the documented grammar of a stock
# reference at a frozen pin.

# Auth-mode vocabulary. The numbers are load-bearing and are asserted against
# the source enum by `test-tunnels-conf.sh`.
readonly ELS2_AUTH_NONE=0
readonly ELS2_AUTH_DH=1
readonly ELS2_AUTH_PSK=2

# The encrypted LeaseSet2 store type. `3` is the standard LeaseSet2 that the
# `.b32.i2p` authority row uses, and is emitted for that row so the same writer
# covers both.
readonly ELS2_STORE_TYPE_STANDARD=3
readonly ELS2_STORE_TYPE_ENCRYPTED=5

# The auth public key is a `Tag<32>`; 32 bytes is 64 lowercase hex.
readonly ELS2_AUTH_KEY_BYTES=32

# The closed set of keys a section may carry. `deny_unknown_fields`-style: an
# unrecognized key is refused rather than ignored, because i2pd ignores unknown
# keys silently and a mistyped key would otherwise produce a lane that looks
# configured and is not.
readonly ELS2_SECTION_BASE_KEYS=(
  type host port keys inbound.length outbound.length
  i2cp.leaseSetType i2cp.leaseSetAuthType
)
readonly ELS2_SECTION_CLIENT_KEYS=(
  i2cp.leaseSetClient.psk i2cp.leaseSetClient.dh
)

els2_group_for_auth_type() {
  # 1 -> .dh, 2 -> .psk, anything else -> nothing.
  case "$1" in
    "${ELS2_AUTH_DH}") printf 'i2cp.leaseSetClient.dh' ;;
    "${ELS2_AUTH_PSK}") printf 'i2cp.leaseSetClient.psk' ;;
    *) return 1 ;;
  esac
}

els2_hex_to_base64() {
  # hex (64 lowercase chars, no prefix) -> base64 with padding, in the
  # **I2P alphabet**. i2pd's table ends `...89-~` (`libi2pd/Base.cpp` `T64`),
  # where standard base64 has `+/`. Its decoder maps unknown characters to -1
  # and completes anyway, so a standard-alphabet value carrying `+` or `/`
  # decodes to the wrong 32 bytes *without an error*: `FromBase64` still
  # succeeds on length, i2pd logs "1 auth keys read", and the destination
  # publishes under a key the lane never held. Found by executing Plan 381
  # WP4: a random PSK whose standard base64 contained `+`/`/` failed live
  # with `ClientCredentialRejected` on the correct key, while the
  # writer↔validator pair agreed with each other throughout — the same
  # self-agreement shape as finding 7. `tr` after encoding is exact: `-`/`~`
  # never appear in standard output except as `+`/`/` substitutes. The `--`
  # is load-bearing: without it `tr` reads `-~` as an option.
  printf '%s' "$1" | xxd -r -p | base64 -w0 | tr -- '+/' '-~'
}

els2_i2pd_b64_to_standard() {
  # The inverse, for validation only: translate back before `base64 -d`.
  printf '%s' "$1" | tr -- '-~' '+/'
}

# `keys` must be a **bare filename**, and this is a reference constraint rather
# than a style choice. i2pd resolves it through `i2p::fs::DataDirPath`, which
# *prepends* the data dir to whatever components it is given
# (`libi2pd/FS.h:175-181`), and `ClientContext::LoadPrivateKeys` then opens that
# concatenated path (`libi2pd_client/ClientContext.cpp:280`). An absolute path
# therefore becomes `<datadir>//abs/...`, fails to open, and i2pd *silently
# creates a brand-new key pair there* -- so the destination comes up with a
# different identity than the lane believes it configured, and every later
# address derivation is wrong for a reason that looks like a crypto defect.
# Plan 214 uses a bare filename for the same reason.
#
# Found by executing Plan 381 WP2: the first lane probe passed an absolute
# path, and the validator accepted it because it only compared the emitted
# value against the same string the caller supplied. Comparing a value against
# the value it came from is not a check.
els2_is_bare_filename() {
  local candidate="$1"
  [[ -n "${candidate}" ]] || return 1
  [[ "${candidate}" == */* ]] && return 1
  [[ "${candidate}" == "." || "${candidate}" == ".." ]] && return 1
  [[ "${candidate}" =~ [[:space:]] ]] && return 1
  # No traversal, and no hidden/shell-ish characters that a config parser and a
  # filesystem could disagree about.
  [[ "${candidate}" =~ ^[A-Za-z0-9._-]+$ ]] || return 1
  [[ "${candidate}" == *".."* ]] && return 1
  return 0
}

els2_is_tcp_port() {
  local candidate="$1"
  [[ "${candidate}" =~ ^[0-9]{1,5}$ ]] || return 1
  (( candidate >= 1 && candidate <= 65535 )) || return 1
  return 0
}

# write_els2_tunnels_conf <path> <section-name> <app-port> <keys-file>
#                         <store-type> <auth-type> [client-key-hex]
#
# `client-key-hex` is required exactly when <auth-type> is DH or PSK, and must
# be absent when it is NONE. A NONE section emits no client key line at all:
# emitting one would be a claim the destination does not act on.
write_els2_tunnels_conf() {
  local path="$1" name="$2" app_port="$3" keys_file="$4"
  local store_type="$5" auth_type="$6" key_hex="${7:-}"

  if ! els2_is_tcp_port "${app_port}"; then
    echo "els2: app port must be an integer in 1-65535, got '${app_port}'" >&2
    return 1
  fi
  if ! els2_is_bare_filename "${keys_file}"; then
    echo "els2: keys must be a bare filename, got '${keys_file}'" >&2
    echo "     (i2pd prepends the data dir via DataDirPath, so a path silently" >&2
    echo "      becomes a different file and a new key pair)" >&2
    return 1
  fi

  case "${store_type}" in
    "${ELS2_STORE_TYPE_STANDARD}"|"${ELS2_STORE_TYPE_ENCRYPTED}") ;;
    *) echo "els2: store type must be 3 or 5, got '${store_type}'" >&2; return 1 ;;
  esac
  case "${auth_type}" in
    "${ELS2_AUTH_NONE}"|"${ELS2_AUTH_DH}"|"${ELS2_AUTH_PSK}") ;;
    *) echo "els2: auth type must be 0, 1 or 2, got '${auth_type}'" >&2; return 1 ;;
  esac

  local group=''
  if [[ "${auth_type}" != "${ELS2_AUTH_NONE}" ]]; then
    group="$(els2_group_for_auth_type "${auth_type}")" || {
      echo "els2: no client key group for auth type ${auth_type}" >&2; return 1; }
    if [[ "${#key_hex}" -ne $((ELS2_AUTH_KEY_BYTES * 2)) ]]; then
      echo "els2: client key must be $((ELS2_AUTH_KEY_BYTES * 2)) hex chars, got ${#key_hex}" >&2
      return 1
    fi
  elif [[ -n "${key_hex}" ]]; then
    echo "els2: auth type NONE must not carry a client key" >&2; return 1
  fi

  {
    printf '[%s]\n' "${name}"
    printf 'type = server\n'
    printf 'host = 127.0.0.1\n'
    printf 'port = %s\n' "${app_port}"
    printf 'keys = %s\n' "${keys_file}"
    printf 'inbound.length = 0\n'
    printf 'outbound.length = 0\n'
    printf 'i2cp.leaseSetType = %s\n' "${store_type}"
    printf 'i2cp.leaseSetAuthType = %s\n' "${auth_type}"
    if [[ -n "${group}" ]]; then
      # The `0:` prefix is discarded by i2pd; the colon's presence is what
      # makes the entry count at all. See the header.
      printf '%s.0 = 0:%s\n' "${group}" "$(els2_hex_to_base64 "${key_hex}")"
    fi
  } > "${path}"
}

# The body of one section, excluding the section header and any other section.
els2_section_body() {
  local section="$1" path="$2"
  awk -v header="[${section}]" '
    $0 == header { flag = 1; next }
    /^\[/{ flag = 0; next }
    flag { print }
  ' "${path}"
}

# validate_els2_tunnels_conf <path> <section-name> <app-port> <keys-file>
#                            <store-type> <auth-type> [client-key-hex]
#
# Non-zero on any violation. Prints one line per violation to stderr so a lane
# failure names the reason rather than just the file.
validate_els2_tunnels_conf() {
  local path="$1" name="$2" app_port="$3" keys_file="$4"
  local store_type="$5" auth_type="$6" key_hex="${7:-}"
  local rc=0

  _bad() { echo "  invalid: $1" >&2; rc=1; }

  # --- the caller's own arguments ------------------------------------------
  # These are checked first, and separately from the file, because the
  # comparisons below are all of the form "the file says X and the caller said
  # X". If the caller is wrong, those comparisons agree with each other and
  # the config is validated into being wrong. Plan 381 WP2 tripped exactly
  # this: an absolute `keys` path and a swapped port argument produced a
  # "valid" config that i2pd could not provision.
  if ! els2_is_tcp_port "${app_port}"; then
    _bad "caller app port must be an integer in 1-65535, got '${app_port}'"
  fi
  if ! els2_is_bare_filename "${keys_file}"; then
    _bad "caller keys must be a bare filename, got '${keys_file}'"
  fi

  if [[ ! -s "${path}" ]]; then
    _bad "tunnels.conf is missing or empty: ${path}"
    return 1
  fi

  local sections body total
  total="$(grep -cE '^\[' "${path}" || true)"
  sections="$(grep -cF "[${name}]" "${path}" || true)"
  if [[ "${total}" != "1" ]]; then _bad "expected exactly 1 section, found ${total}"; fi
  if [[ "${sections}" != "1" ]]; then _bad "expected exactly 1 [${name}] header, found ${sections}"; fi
  [[ "${rc}" -eq 0 ]] || return 1

  body="$(els2_section_body "${name}" "${path}")"

  # --- the documented base keys ------------------------------------------
  grep -qxF 'type = server' <<<"${body}" || _bad "type must be 'server' (transparent), so i2pd is the endpoint and not a transform"
  grep -qxF 'host = 127.0.0.1' <<<"${body}" || _bad "host must be loopback"
  grep -qxF "port = ${app_port}" <<<"${body}" || _bad "port must be ${app_port}"
  grep -qxF "keys = ${keys_file}" <<<"${body}" || _bad "keys must be ${keys_file}"

  # --- the emitted values, checked on their own terms -----------------------
  # Read back out of the file rather than compared against the arguments, so a
  # hand-edited config is caught even when the caller repeats the same mistake.
  local observed_port observed_keys
  observed_port="$(sed -n -E 's/^[[:space:]]*port[[:space:]]*=[[:space:]]*(.*[^[:space:]])[[:space:]]*$/\1/p' <<<"${body}" | head -1)"
  observed_keys="$(sed -n -E 's/^[[:space:]]*keys[[:space:]]*=[[:space:]]*(.*[^[:space:]])[[:space:]]*$/\1/p' <<<"${body}" | head -1)"
  if ! els2_is_tcp_port "${observed_port}"; then
    _bad "emitted port is not a TCP port: '${observed_port}'"
  fi
  if ! els2_is_bare_filename "${observed_keys}"; then
    _bad "emitted keys is not a bare filename: '${observed_keys}'"
  fi
  grep -qxF 'inbound.length = 0' <<<"${body}" || _bad "inbound.length must be 0"
  grep -qxF 'outbound.length = 0' <<<"${body}" || _bad "outbound.length must be 0"
  grep -qxF "i2cp.leaseSetType = ${store_type}" <<<"${body}" || _bad "i2cp.leaseSetType must be ${store_type}"
  grep -qxF "i2cp.leaseSetAuthType = ${auth_type}" <<<"${body}" || _bad "i2cp.leaseSetAuthType must be ${auth_type}"

  # --- deny_unknown_fields ------------------------------------------------
  # i2pd ignores a key it does not know, so a typo would leave the lane looking
  # configured and behaving otherwise. Every key must be on the allow-set.
  #
  # The client keys are a *group*: `ReadI2CPOptionsGroup` is a prefix match, so
  # `i2cp.leaseSetClient.psk` and `i2cp.leaseSetClient.psk.0` are both live
  # spellings. A trailing `.<digits>` is therefore normalized away before the
  # lookup -- matching on the raw key instead would have rejected the indexed
  # spelling the reference accepts, which is precisely the mistake this plan
  # exists to prevent.
  local observed key base allowed
  allowed=" ${ELS2_SECTION_BASE_KEYS[*]} ${ELS2_SECTION_CLIENT_KEYS[*]} "
  while IFS= read -r key; do
    [[ -z "${key}" ]] && continue
    base="$(printf '%s' "${key}" | sed 's/\.[0-9][0-9]*$//')"
    case "${allowed}" in
      *" ${base} "*) ;;
      *) _bad "unknown key in section: '${key}'" ;;
    esac
  done < <(sed -n 's/^[[:space:]]*\([A-Za-z0-9._]*\)[[:space:]]*=.*/\1/p' <<<"${body}")

  # --- the client key, if the mode calls for one --------------------------
  local group='' expect_group=''
  if [[ "${auth_type}" != "${ELS2_AUTH_NONE}" ]]; then
    group="$(els2_group_for_auth_type "${auth_type}")" || return 1
    # Presence is matched on the group *prefix*, with or without an index,
    # because that is exactly what the reference does: `ReadI2CPOptionsGroup`
    # and `ReadAuthKey` both test only `key.compare(0, group.length(), group)`.
    # Requiring the `.0` spelling here would refuse a config the reference
    # accepts; accepting any suffix would be looser than the reference and
    # would let a mistyped key through, so the index is constrained to digits.
    expect_group="${group}"
    if ! grep -qE "^${expect_group}(\.[0-9]+)?[[:space:]]*=" <<<"${body}"; then
      _bad "auth type ${auth_type} requires an ${group} entry"
    else
      local value
      value="$(sed -n -E "s/^${expect_group}(\.[0-9]+)?[[:space:]]*=[[:space:]]*//p" <<<"${body}" | head -1)"
      # The colon is mandatory and is the single easiest thing to get wrong.
      case "${value}" in
        *:*) ;;
        *) _bad "${expect_group} value has no ':' separator; i2pd would drop it silently" ;;
      esac
      local b64="${value#*:}"
      # The reference reads this value under its own alphabet, so a
      # standard-alphabet `+` or `/` here is not a spelling choice: i2pd
      # decodes it to different bytes than written, silently. Refuse it
      # rather than validating a line the reference interprets otherwise.
      case "${b64}" in
        *+*|*/*) _bad "${expect_group} must use the I2P base64 alphabet ('-~', not '+/'); i2pd would decode other bytes than written" ;;
      esac
      local decoded
      decoded="$(els2_i2pd_b64_to_standard "${b64}" | base64 -d 2>/dev/null | wc -c | tr -d ' ')"
      if [[ "${decoded}" != "${ELS2_AUTH_KEY_BYTES}" ]]; then
        _bad "${expect_group} must decode to ${ELS2_AUTH_KEY_BYTES} bytes, got ${decoded}"
      fi
    fi
    # The *other* group's absence matters too: an i2pd that read both would
    # accept two credentials for one destination.
    local other
    for other in "${ELS2_SECTION_CLIENT_KEYS[@]}"; do
      [[ "${other}" == "${group}" ]] && continue
      if grep -qE "^${other}(\.0)?[[:space:]]*=" <<<"${body}"; then
        _bad "auth type ${auth_type} must not also carry ${other}"
      fi
    done
  else
    for group in "${ELS2_SECTION_CLIENT_KEYS[@]}"; do
      if grep -qE "^${group}(\.0)?[[:space:]]*=" <<<"${body}"; then
        _bad "auth type NONE must not carry ${group}"
      fi
    done
  fi

  # --- an unresolved template place-holder --------------------------------
  if grep -qE '\$\{[A-Za-z_][A-Za-z0-9_]*\}|\{\{[A-Za-z_][A-Za-z0-9_]*\}\}|<%[A-Za-z_][A-Za-z0-9_]*%>|__[A-Za-z_][A-Za-z0-9_]*__' "${path}"; then
    _bad "unresolved template place-holder in ${path}"
  fi

  return "${rc}"
}