# Terraform (Azure) idiom reference

Condensed, cited reference for writing Terraform in this repository (`infra/`). Prefer a concrete rule over vague advice. This file lives at the `skills/` level, referenced by `writing-terraform` as `../references/terraform-idioms.md`. Complements, doesn't duplicate, `writing-dockerfiles`'s reference — that file owns image content; this file owns the resources that run and configure those images.

## Provider version pinning

Pin `azurerm` with a `required_providers` block in the root module, e.g. `version = "~> 4.2"` (per Azure's own Terraform best-practices guidance, minimum `4.2`, and the [azurerm provider registry](https://registry.terraform.io/providers/hashicorp/azurerm/latest/docs)). `~>` allows patch/minor drift but blocks an accidental major-version jump that could silently change resource schemas. Always pair with a `provider "azurerm" { features {} }` block — the provider errors on startup without it, even with no customization needed.

## Remote state backend

An `azurerm` backend (storage account + blob container) holds state remotely so a solo machine loss doesn't lose the only copy of what's deployed, and so `terraform plan`/`apply` always start from the same source of truth rather than a possibly-stale local `terraform.tfstate`. The `azurerm` backend has always used a native blob lease as its lock, with no separate lock-table resource ever required — unlike the AWS S3 backend, which needed a separate DynamoDB table for locking until Terraform 1.10 added native S3 locking. Stage 12 builds this storage account itself; that one resource is necessarily created *before* the backend block can point at it (a one-time bootstrap, typically applied once with local state or `-backend=false`, then migrated).

## Resource naming and tagging

Azure has real naming constraints that bite if ignored: `azurerm_container_registry` names must be **alphanumeric only, globally unique**; `azurerm_postgresql_flexible_server` names must be globally unique too (both confirmed against the provider's own resource docs). Pick one naming scheme early — e.g. `summa-{resource}-{env}` for most resources, `summacr{unique}` (no hyphens) for the registry — and hold it consistently rather than improvising per-resource. Tag every resource with at minimum a `project = "summa"` tag; tags are how a later `az` CLI or cost query finds "everything Summa" without relying on resource-group scoping alone.

## Secret handling — never plaintext in committed `.tf`

- **Container Registry: skip `admin_enabled`, use a managed identity + `AcrPull` role assignment instead.** The registry docs' own example shows this exact pattern (`azurerm_kubernetes_cluster` → `azurerm_role_assignment` with `role_definition_name = "AcrPull"`, `scope = azurerm_container_registry.example.id`). The same shape applies to ACA: give the Container App a managed identity (`identity { type = "SystemAssigned" }` — note the capitalized `"SystemAssigned"` here), grant it `AcrPull` on the registry, and reference that identity in the app's `registry` block via `identity = "system"` — **lowercase**, a different literal than the `identity` block's own `type` value; confirmed against the ARM `RegistryCredentials.identity` docs, not guessed. Admin credentials are a single shared secret with no per-caller audit trail; a role assignment is scoped, revocable, and needs no secret to leak in the first place.
- **PostgreSQL admin password: never a literal in `.tf`.** `azurerm_postgresql_flexible_server.administrator_password` takes a string — pass it via a `variable` sourced from an environment variable (`TF_VAR_postgres_admin_password`) or a secrets manager, never hardcoded. The [provider's own docs](https://registry.terraform.io/providers/hashicorp/azurerm/latest/docs/resources/postgresql_flexible_server) note this ends up in state as plaintext regardless — remote state (above) plus restricting who can read the state storage account is the actual mitigation, not the variable indirection alone. (The provider also exposes a write-only `administrator_password_wo` + `administrator_password_wo_version` pair that never persists to state at all — a real option if that mitigation isn't enough, at the cost of ephemeral-variable workflow complexity; not adopted for Summa since the state storage account is already locked down.)
- **Container App secrets: ACA's native `secret` block (value from a variable), not Key Vault** — decided for Stage 13 (`docs/decisions.md`, 2026-07-31), revising this doc's earlier Key Vault-first framing. Key Vault sounds like it removes the secret from Terraform state, but it doesn't: the Postgres password is already in state via `azurerm_postgresql_flexible_server.main` regardless of how ACA reads it, unless the Key Vault secret is populated *entirely outside Terraform* — real operational complexity, not a config flag. Reach for `key_vault_secret_id` + `identity` instead of inline `value` only once there's an actual reason (secret rotation, sharing one secret across more consumers than fit cleanly in Terraform state already) — not by default.
- **`public_network_access_enabled` — decide it deliberately, don't leave the default.** `azurerm_postgresql_flexible_server` defaults this to `true`. For Summa's current no-VNet setup this likely needs to stay `true` for the API/worker to reach it (no `delegated_subnet_id` yet, per the build plan) — but that's a call to make explicitly, not inherit silently. Note it as a real decision if the answer isn't "leave the default."

## Module vs. flat file structure

**Modules from Stage 12, confirmed.** `docs/architecture.md` records Terraform "in modules so a later ACA→AKS swap is localized" as a deliberate decision, not aspirational language — and `docs/decisions.md` (2026-07-30) confirms it stands as written, as an explicit exception to the general "no speculative abstraction" rule (this is a specific, already-recorded call, not new scaffolding introduced mid-stage). Structure: `infra/modules/<concern>/` (e.g. one module per resource group of related resources — registry, database, container-apps-environment — decide the exact boundary as part of each stage's plan) composed from root-level `infra/main.tf`, `variables.tf`, `outputs.tf`.

## Container Apps basics (Stage 13)

- **Revisions.** `revision_mode = "Single"` keeps exactly one active revision (simplest — a new `apply` replaces it in place); `"Multiple"` allows traffic-split rollouts via `traffic_weight` blocks in `ingress`, at the cost of that extra complexity. `"Single"` is the right starting point unless a rollout strategy is actually needed.
- **API vs. worker shape.** The API app needs an `ingress` block (`external_enabled = true`, a `target_port` matching the container's listen port, one `traffic_weight` block summing to exactly `100`). The worker app omits `ingress` entirely — no inbound traffic, matches `docs/build-plan.md`'s Stage 13 "no ingress, always-on" description.
- **Scaling.** `template.min_replicas`/`max_replicas` control the Consumption-plan autoscale range; the API can scale to `0` if idle cost matters, the worker should stay at `min_replicas = 1` so it never stops polling.
- **CPU/memory pairing.** On the default Consumption workload profile, a container's `cpu` and `memory` values must add up to one of Azure's fixed combinations (e.g. `0.25` vCPU + `0.5Gi`) — not arbitrary numbers; check the current combination table before picking values.

## Anti-patterns to flag on review

- A literal secret (password, connection string, access key) anywhere in a `.tf` file, even one that "looks like a placeholder."
- `admin_enabled = true` on `azurerm_container_registry` when a managed identity + `AcrPull` role assignment would do.
- No `required_providers` version constraint, or a constraint loose enough to allow a major-version jump (bare `>= 4.0` with no upper bound).
- Local (un-configured) backend for anything beyond the one-time bootstrap apply that creates the remote-state storage account itself.
- Flat `.tf` files with no `infra/modules/` structure from Stage 12 onward — this repo's module boundary is a confirmed decision, not an optional-until-needed abstraction.
- `public_network_access_enabled` (or equivalent) left at its provider default without a comment/decision recording *why* that default is correct here.
- Hardcoded `location`/`resource_group_name` repeated per-resource instead of referencing the one `azurerm_resource_group` resource's attributes.
- Stage 12 resources provisioned with no budget alert configured — an explicit acceptance criterion, not optional polish.
