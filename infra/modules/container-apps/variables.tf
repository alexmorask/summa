variable "name_prefix" {
  description = "Prefix for names of resources created by this module."
  type        = string
  default     = "summa"
}

variable "resource_group_name" {
  description = "Resource group to create these resources in."
  type        = string
}

variable "location" {
  description = "Azure region."
  type        = string
}

variable "registry_id" {
  description = "Resource ID of the Container Registry, for the AcrPull role assignments."
  type        = string
}

variable "registry_login_server" {
  description = "Login server of the Container Registry, for image references."
  type        = string
}

variable "connection_string" {
  description = "Postgres connection string, wired into both apps as a Container App secret."
  type        = string
  sensitive   = true
}

variable "image_tag" {
  description = "Tag to pull for both the API and worker images."
  type        = string
  default     = "latest"
}

variable "tags" {
  description = "Tags applied to every resource in this module."
  type        = map(string)
  default     = {}
}
