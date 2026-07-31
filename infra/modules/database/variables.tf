variable "name" {
  description = "Globally-unique PostgreSQL Flexible Server name."
  type        = string
}

variable "resource_group_name" {
  description = "Resource group to create the server in."
  type        = string
}

variable "location" {
  description = "Azure region."
  type        = string
}

variable "administrator_login" {
  description = "Postgres admin username."
  type        = string
}

variable "administrator_password" {
  description = "Postgres admin password."
  type        = string
  sensitive   = true
}

variable "admin_ip_address" {
  description = "Public IP address allowed to connect for manual admin/migration access."
  type        = string
}

variable "tags" {
  description = "Tags applied to the server."
  type        = map(string)
  default     = {}
}
