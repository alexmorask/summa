variable "auth_audience" {
  description = "Expected JWT audience for the api container — the API's own App Registration identifier URI."
  type        = string
}

variable "auth_authority" {
  description = "OIDC authority URL for JWT Bearer validation on the api container, e.g. https://login.microsoftonline.com/{tenant}/v2.0."
  type        = string
}

variable "connection_string" {
  description = "Postgres connection string, wired into both apps as a Container App secret."
  type        = string
  sensitive   = true
}

variable "location" {
  description = "Azure region."
  type        = string
}

variable "postgres_admin_login" {
  description = "Postgres admin login, for the migration job's PGUSER."
  type        = string
}

variable "postgres_admin_password" {
  description = "Postgres admin password, for the migration job's PGPASSWORD."
  type        = string
  sensitive   = true
}

variable "postgres_database" {
  description = "Postgres database name, for the migration job's PGDATABASE."
  type        = string
}

variable "postgres_host" {
  description = "Postgres server FQDN, for the migration job's PGHOST."
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

variable "resource_group_name" {
  description = "Resource group to create these resources in."
  type        = string
}

variable "image_tag" {
  description = "Tag to pull for both the API and worker images."
  type        = string
  default     = "latest"
}

variable "name_prefix" {
  description = "Prefix for the shared Container App Environment and Log Analytics Workspace names."
  type        = string
  default     = "summa"
}

variable "tags" {
  description = "Tags applied to every resource in this module."
  type        = map(string)
  default     = {}
}
