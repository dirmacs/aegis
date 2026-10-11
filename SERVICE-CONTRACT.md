# Aegis secrets-as-service — contract

Owner direction: build the secrets keychain as a **service** (secrets held in the
container, agents authenticate over the wire), which makes the local-vault
`chmod` defect moot by design. Then extend `aegis-net` into a working SDN.

Order: containerize (DONE) → secrets-as-service (this) → net/SDN.

## Step 1 — containerize: DONE

`gh`/merge state was verified, not assumed. PR #1 `docker: add Dockerfile
(phase 2 dockerization)` was reported MERGED, but `origin/main` (a
remote-tracking ref) still showed `7767645` with no Dockerfile in its tree,
and no deletion commit existed in history. The honest reading of that is
"the merge lost its content" — but the object `fc0ede1c` named by the PR was
not present locally, so `git ls-tree` was answering about nothing. `git fetch
origin main` resolved it: main is at `fc0ede1c`, the Dockerfile is there, PR
#1 landed correctly. Recorded because it was a near-miss on a false claim.

Image built and smoke-tested from that Dockerfile:

| check | result |
|---|---|
| build | `aegis:fc0ede1`, 280 MB, rc=0, BuildKit cache mounts |
| non-root runtime | `uid=1000(aegis) gid=1000(aegis)` |
| subcommands | `aegis --help` lists 17 |
| version | `aegis 0.1.0` |

Note the Dockerfile's own boundary: `sync`/`enforce`/`bootstrap`/`net` mutate
host state and are documented to run on the host, not in the container. That
constrains step 3 — the SDN cannot fully live in this image.

## Step 2 — secrets-as-service

### Why hyper and not axum
`Cargo.lock` already resolves `hyper 1.8.1` and `tower 0.5.3` (transitively).
Adding axum would pull a large new subgraph into a security-sensitive
service. Five routes do not justify that. **Use hyper directly.**

### Environment contract (authoritative)
Shared by the service crate and the compose file that follows.

| var | meaning | default |
|---|---|---|
| `AEGIS_VAULT_PASSWORD` | master password, REQUIRED | — |
| `AEGIS_TOKENS` | `agent:token,agent:token`, REQUIRED | — |
| `AEGIS_VAULT_PATH` | vault file path | `/data/vault.json` |
| `AEGIS_BIND` | bind address | `0.0.0.0:8477` |

### Routes
`GET /healthz` (unauthenticated, version + status only); `GET /api/v1/secrets`
(keys + tags, never values); `GET /api/v1/secrets/{key}`; `PUT
/api/v1/secrets/{key}`; `DELETE /api/v1/secrets/{key}`.

### Fail-closed startup (the core property)
Blank/missing `AEGIS_VAULT_PASSWORD` or `AEGIS_TOKENS` → non-zero exit, **no
listener bound**. An empty `AEGIS_TOKENS` must never yield an auth map that
accepts an empty bearer token. Each failure mode gets its own test.

### Corrections to the pre-work survey
The survey's "no headless path" was wrong in mechanism, right in effect:
`SecretStore::open(path, master_password: &str)` already takes the password as
a string, so the **library** was always headless-capable. The interactivity
is one `dialoguer::Password::new()` call in `crates/aegis-cli/src/commands/
secrets.rs:58`. Also: the CLI binary is `aegis-cli`, not `aegis` — a
grep for `secrets` in `crates/aegis/src/main.rs` returns nothing for that
reason.

## Step 3 — net / SDN
Separate slice. `aegis-net` currently evaluates firewall rules and generates
WireGuard config but nothing brings an interface up, distributes configs, or
applies the firewall. Risk already flagged to the owner once and accepted: a
wrong `AllowedIPs` silently drops traffic and presents as a network fault.
