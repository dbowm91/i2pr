# Current external consumer conformance matrix (Plan 379)

The fixture is a standalone Cargo package, independent of the i2pr workspace graph.
Its test source uses only the public API from the exact Plan 379 Git revision in
`Cargo.toml`.

The first eight rows are the Plan 351 matrix, carried forward byte-for-byte. They
prove the policy/filter surface an external consumer could already reach has not
regressed. The last three are new and exist because the Plan 359 amendment added
the one permitted `i2pr-proto` edge, which no external consumer had compiled
against before.

| Contract | Evidence |
| --- | --- |
| Generic client/server construction, bounded IDs, endpoint validation | `generic_client_and_server_validate_through_public_values` |
| Dedicated groups differ; explicit shared client/server group agrees | `explicit_destination_groups_preserve_dedicated_and_shared_identity_domains` |
| Authenticated peer hash, deny policy, and adapter monotonic time for rate limits | `access_and_rate_policy_use_authenticated_hash_and_adapter_clock` |
| HTTP client privacy rewrite and server Host/hop metadata filtering | `http_client_privacy_and_server_filters_are_reused` |
| SOCKS5 CONNECT and IRC privacy/parser behavior | `socks_connect_parser_and_irc_privacy_filter_are_publicly_usable` |
| Malformed and max+1 bounded typed failures | `malformed_and_max_plus_one_inputs_keep_typed_bounded_failures` |
| Deterministic generation diff and explicit lifecycle updates | `generation_diff_is_deterministic_and_adapter_lifecycle_is_explicit` |
| Start failure does not mutate policy generation; restart from validated public spec | `failed_adapter_start_is_visible_and_does_not_change_policy_generation` |
| **A valid b33 parses to the encrypted-service variant and exposes a structured, canonical address** — narrow (56), wide (60), and both unblinded signing types | `encrypted_service_destination_is_usable_from_outside_the_workspace` |
| **A b33-shaped failure is reported as an encrypted-service problem, never as a Base32 length error** | `an_encrypted_service_address_is_not_reported_as_a_malformed_base32_label` |
| **A `StaticAliasTable` refuses an encrypted-service target; ordinary and alias-to-alias targets still work and the table does not resolve** | `an_encrypted_service_target_is_contained_outside_static_aliases` |
| No daemon/runtime/testkit/private source dependencies; exact Git pin; only `i2pr-proto` beyond the package itself | `bash scripts/check-portable-service-tunnel-consumer.sh` |

The fake adapter models metadata and lifecycle only. It does not open sockets,
create SAM sessions, or implement transport behavior.