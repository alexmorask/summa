variable "admin_object_id" {
  description = "Fixed Entra object ID of the human administrator who owns this App Registration and Service Principal — not derived from whoever is currently running Terraform, since that resolves differently for a local human apply vs. a CD service-principal apply and would fight itself every run."
  type        = string
}

variable "github_repository_id" {
  description = "GitHub repository allowed to authenticate, in GitHub's immutable OIDC subject format: \"owner@owner_id/repo@repo_id\" — not just \"owner/repo\", since new repos default to immutable subject claims keyed on GitHub's numeric IDs, not names."
  type        = string
}

variable "key_vault_id" {
  description = "Key Vault resource ID, for the read-only secrets grant scoped to it — CD's own terraform plan/apply refreshes this vault's secret directly, since it lives in the same root CD applies."
  type        = string
}

variable "registry_id" {
  description = "Container Registry resource ID, for the role-assignment-management grant scoped to it."
  type        = string
}

variable "resource_group_id" {
  description = "Resource group the service principal gets Contributor on."
  type        = string
}

variable "tfstate_storage_account_id" {
  description = "Storage account the service principal gets Storage Blob Data Contributor on, for remote Terraform state."
  type        = string
}
