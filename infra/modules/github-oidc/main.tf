resource "azuread_application" "github_actions" {
  display_name = "summa-github-actions"
  owners       = [var.admin_object_id]
}

resource "azuread_service_principal" "github_actions" {
  client_id = azuread_application.github_actions.client_id
  owners    = [var.admin_object_id]
}

resource "azuread_application_federated_identity_credential" "github_actions" {
  application_id = azuread_application.github_actions.id
  display_name   = "summa-production-environment"
  description    = "CD workflow runs deploying to the production GitHub Environment."
  audiences      = ["api://AzureADTokenExchange"]
  issuer         = "https://token.actions.githubusercontent.com"
  subject        = "repo:${var.github_repository_id}:environment:production"
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

resource "azurerm_role_assignment" "github_actions_acr_role_admin" {
  scope                            = var.registry_id
  role_definition_name             = "Role Based Access Control Administrator"
  principal_id                     = azuread_service_principal.github_actions.object_id
  principal_type                   = "ServicePrincipal"
  skip_service_principal_aad_check = true

  condition_version = "2.0"
  condition         = <<-EOT
    ((!(ActionMatches{'Microsoft.Authorization/roleAssignments/write'})) OR (@Request[Microsoft.Authorization/roleAssignments:RoleDefinitionId] ForAnyOfAnyValues:GuidEquals{7f951dda-4ed3-4680-a7ca-43fe172d538d}))
    AND
    ((!(ActionMatches{'Microsoft.Authorization/roleAssignments/delete'})) OR (@Resource[Microsoft.Authorization/roleAssignments:RoleDefinitionId] ForAnyOfAnyValues:GuidEquals{7f951dda-4ed3-4680-a7ca-43fe172d538d}))
  EOT
}

data "azuread_application_published_app_ids" "well_known" {}

resource "azuread_service_principal" "msgraph" {
  client_id    = data.azuread_application_published_app_ids.well_known.result.MicrosoftGraph
  use_existing = true
}

resource "azuread_app_role_assignment" "github_actions_graph_read" {
  app_role_id         = azuread_service_principal.msgraph.app_role_ids["Application.Read.All"]
  principal_object_id = azuread_service_principal.github_actions.object_id
  resource_object_id  = azuread_service_principal.msgraph.object_id
}
