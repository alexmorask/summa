data "azuread_client_config" "current" {}

resource "azuread_application" "github_actions" {
  display_name = "summa-github-actions"
  owners       = [data.azuread_client_config.current.object_id]
}

resource "azuread_service_principal" "github_actions" {
  client_id = azuread_application.github_actions.client_id
  owners    = [data.azuread_client_config.current.object_id]
}

resource "azuread_application_federated_identity_credential" "github_actions" {
  application_id = azuread_application.github_actions.id
  display_name   = "summa-production-environment"
  description    = "CD workflow runs deploying to the production GitHub Environment."
  audiences      = ["api://AzureADTokenExchange"]
  issuer         = "https://token.actions.githubusercontent.com"
  subject        = "repo:${var.github_repository}:environment:production"
}

resource "azurerm_role_assignment" "github_actions_contributor" {
  scope                            = var.resource_group_id
  role_definition_name             = "Contributor"
  principal_id                     = azuread_service_principal.github_actions.object_id
  principal_type                   = "ServicePrincipal"
  skip_service_principal_aad_check = true
}

resource "azurerm_role_assignment" "github_actions_tfstate" {
  scope                            = var.tfstate_storage_account_id
  role_definition_name             = "Storage Blob Data Contributor"
  principal_id                     = azuread_service_principal.github_actions.object_id
  principal_type                   = "ServicePrincipal"
  skip_service_principal_aad_check = true
}
