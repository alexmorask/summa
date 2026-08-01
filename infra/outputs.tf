output "resource_group_name" {
  description = "Name of the resource group holding all Stage 12 resources."
  value       = azurerm_resource_group.main.name
}

output "acr_login_server" {
  description = "Container Registry login server, for Stage 11/13 image push/pull."
  value       = module.registry.login_server
}

output "postgres_fqdn" {
  description = "Fully-qualified domain name of the Postgres server."
  value       = module.database.fqdn
}

output "postgres_server_name" {
  description = "Postgres Flexible Server name."
  value       = module.database.server_name
}

output "tfstate_storage_account_name" {
  description = "Storage account holding remote Terraform state, once migrated."
  value       = azurerm_storage_account.tfstate.name
}

output "tfstate_container_name" {
  description = "Blob container holding remote Terraform state, once migrated."
  value       = azurerm_storage_container.tfstate.name
}

output "api_url" {
  description = "Public URL of the deployed API."
  value       = "https://${module.container_apps.api_fqdn}"
}

output "github_actions_client_id" {
  description = "App Registration client ID for the GitHub Actions OIDC identity — set as the production Environment's AZURE_CLIENT_ID variable."
  value       = module.github_oidc.client_id
}
