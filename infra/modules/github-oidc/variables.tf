variable "github_repository" {
  description = "GitHub repository allowed to authenticate, as \"owner/repo\"."
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
