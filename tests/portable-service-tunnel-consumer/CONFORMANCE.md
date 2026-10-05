# External consumer conformance matrix

The fixture is a standalone Cargo package, independent of the i2pr workspace graph. Its test source uses only the public API from the exact Plan 350 Git revision in `Cargo.toml`.

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
| No daemon/runtime/testkit/private source dependencies; exact Git pin | `bash scripts/check-portable-service-tunnel-consumer.sh` |

The fake adapter models metadata and lifecycle only. It does not open sockets, create SAM sessions, or implement transport behavior.
