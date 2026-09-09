# ADR-013: Surface Reduction — flux-ui Retirement

**Status:** Accepted
**Date:** 2026-09-09
**Extends:** ADR-006 (Security Hardening), ADR-007 (Universal Connector Framework)

---

## Background

Flux is a free service with no paying customers. The public API at
`api.flux-universe.com` is read-only to consumers and **must remain publicly
reachable** — that is the product. Everything else is operator surface, and the
2026-09-09 audit found the operator surface carries more risk than the product does.

`flux-ui` is a Node service on port 8082, reachable from the LAN, serving a dashboard
nobody depends on. It holds three defects:

1. **Path traversal** — `server.js:172` builds the served path as
   `path.join(__dirname, 'public', req.url)` with no sanitization. `path.join`
   collapses `..`, so a request escapes the webroot.
2. **Unauthenticated connector-manager proxy** — `server.js:107` forwards
   `/api/connector-manager/*` to connector-manager, which has **no auth of its own**
   (`connector-manager/src/api.rs:406`). Any LAN host can list, create, and delete
   connectors. The Singer runner spawns `config.tap_name` as a command
   (`runners/named.rs:275`), so this is remote command execution.
3. **Dead loadtest endpoints** — `server.js:33-98` accept unauthenticated POSTs that
   `ssh` to a hardcoded `etl@192.168.50.40` and run a script against
   `flux.eckman-tech.com`, a retired host.

The Ratzilla WASM UI (`ui/src/main.rs`, `ui/index.html`, `ui/dist/`) is tracked in git
but dead — the Dockerfile builds the Node `server.js` path. `ui/target/` is 230 MB.

Retiring flux-ui removes all three defects by deletion. But it also removes the **only**
access path to connector-manager, which runs 29 live Bento connectors: port 3001 is
Docker-internal and reachable no other way. The two changes must land together.

## Decisions

### 1. Retire flux-ui entirely

Remove the service from `docker-compose.yml` and delete `ui/`. Three defects vanish
rather than being patched, and a Node service, its dependencies, and 230 MB of dead
build artifacts leave the tree.

### 2. Bind connector-manager to 127.0.0.1:3001

Publish the port on the loopback interface only. Reachable via `curl localhost:3001`
over SSH; unreachable from the LAN and from the Cloudflare tunnel.

This is strictly **less** exposed than today, where the UI proxy hands the LAN
unauthenticated write access to it. It is not a substitute for authenticating
connector-manager, which remains unauthenticated and should not be re-exposed
without one.

### 3. Connector management is an SSH-and-curl operation

No UI replacement. Namespace and connector creation happen through Claude with VM
access (see CLAUDE.md, VM Ownership) or by hand over SSH. Sole operator, free service,
low change rate — a dashboard is not worth a network service to secure.

### 4. The loadtest capability is deleted, not ported

It targets retired infrastructure, is unauthenticated, and shells out over SSH. No
replacement. Load testing, if wanted again, belongs in a script run deliberately by an
operator, not an endpoint reachable from the network.

### 5. The public API stays public and read-only

`api.flux-universe.com` keeps its current reachability. Nothing in this ADR touches it.
Writes remain namespace-token gated; reads remain open by design.

## Constraints

- **Do not retire flux-ui before connector-manager is rebound.** The single operation
  must not leave the 29 live connectors unmanageable.
- **Do not expose connector-manager beyond loopback** without first giving it real
  authentication, in its own ADR.
- **Do not treat loopback binding as authentication.** Anyone with a shell on .107 has
  full connector-manager access; that is acceptable only because .107 has one operator.
- **Do not remove or restrict the public read API** to reduce surface. It is the product.
- **Do not re-add a dashboard** without an ADR that addresses authentication first.

## References

- ADR-006: Security Hardening
- ADR-007: Universal Connector Framework
- ADR-012: Connector Reliability and Liveness
- Audit session 2026-09-09
