variable "location" {
  description = "Azure region."
  type        = string
}

variable "name" {
  description = "Globally-unique Container Registry name (alphanumeric only, no hyphens)."
  type        = string
}

variable "resource_group_name" {
  description = "Resource group to create the registry in."
  type        = string
}

variable "tags" {
  description = "Tags applied to the registry."
  type        = map(string)
  default     = {}
}
