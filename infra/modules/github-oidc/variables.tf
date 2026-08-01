variable "github_repository_id" {
  description = "GitHub repository allowed to authenticate, in GitHub's immutable OIDC subject format: \"owner@owner_id/repo@repo_id\" — not just \"owner/repo\", since new repos default to immutable subject claims keyed on GitHub's numeric IDs, not names."
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
