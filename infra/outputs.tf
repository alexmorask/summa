output "resource_group_name" {
  description = "Name of the resource group all Summa infrastructure is deployed into, across every layer."
  value       = azurerm_resource_group.main.name
}

output "tfstate_storage_account_name" {
  description = "Storage account holding remote Terraform state for all three layers."
  value       = azurerm_storage_account.tfstate.name
}

output "tfstate_container_name" {
  description = "Blob container holding remote Terraform state for all three layers."
  value       = azurerm_storage_container.tfstate.name
}

output "postgres_admin_password" {
  description = "Postgres admin password, generated here and stored durably in Key Vault (see key_vault_name) — never typed into a TF_VAR again."
  value       = random_password.postgres_admin.result
  sensitive   = true
}

output "key_vault_name" {
  description = "Key Vault holding the Postgres admin password, for manual retrieval: az keyvault secret show --vault-name <this> --name postgres-admin-password"
  value       = azurerm_key_vault.main.name
}

output "acr_login_server" {
  description = "Container Registry login server, for image push/pull."
  value       = module.registry.login_server
}

output "ledger_api_test_client_id" {
  description = "Client ID of the test-client App Registration used for CI and local dev token acquisition against Summa.Ledger.Api."
  value       = module.api_auth.ledger_api_test_client_id
}

output "ledger_api_test_client_secret" {
  description = "Client secret of the test-client App Registration used for CI and local dev token acquisition against Summa.Ledger.Api."
  value       = module.api_auth.ledger_api_test_client_secret
  sensitive   = true
}

output "github_actions_client_id" {
  description = "App Registration client ID for the GitHub Actions OIDC identity — set as the production Environment's AZURE_CLIENT_ID variable."
  value       = module.github_oidc.client_id
}

output "location" {
  description = "Azure region all resources are deployed to."
  value       = var.location
}

output "registry_id" {
  description = "Resource ID of the Container Registry, for AcrPull role assignments in other layers."
  value       = module.registry.id
}

output "tenant_id" {
  description = "Entra ID tenant ID, for JWT Bearer authority configuration in other layers."
  value       = local.tenant_id
}

output "ledger_api_client_id" {
  description = "Client ID of the API's own App Registration, used as the JWT Bearer audience."
  value       = module.api_auth.ledger_api_client_id
}
