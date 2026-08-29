resource "azuread_application" "policy_api" {
  display_name = "policy-api"
  owners       = [var.admin_object_id]

  api {
    requested_access_token_version = 2
  }

  app_role {
    allowed_member_types = ["Application"]
    description          = "Read pricing policies."
    display_name         = "Policy Read"
    enabled              = true
    id                   = "8c0674ca-619f-4c70-815f-4a0e9ddf8ebc"
    value                = "Policy.Read"
  }

  app_role {
    allowed_member_types = ["Application"]
    description          = "Author and publish pricing policies."
    display_name         = "Policy Write"
    enabled              = true
    id                   = "bca8ad60-16e0-420b-8d35-331952b62566"
    value                = "Policy.Write"
  }
}

resource "azuread_service_principal" "policy_api" {
  client_id = azuread_application.policy_api.client_id
  owners    = [var.admin_object_id]
}

resource "azuread_application" "policy_api_test_client" {
  display_name = "policy-api-test-client"
  owners       = [var.admin_object_id]
}

resource "azuread_service_principal" "policy_api_test_client" {
  client_id = azuread_application.policy_api_test_client.client_id
  owners    = [var.admin_object_id]
}

resource "azuread_application_password" "policy_api_test_client" {
  application_id = azuread_application.policy_api_test_client.id
  display_name   = "ci-and-local-dev"
}

resource "azuread_app_role_assignment" "policy_api_test_client_read" {
  app_role_id         = azuread_service_principal.policy_api.app_role_ids["Policy.Read"]
  principal_object_id = azuread_service_principal.policy_api_test_client.object_id
  resource_object_id  = azuread_service_principal.policy_api.object_id
}

resource "azuread_app_role_assignment" "policy_api_test_client_write" {
  app_role_id         = azuread_service_principal.policy_api.app_role_ids["Policy.Write"]
  principal_object_id = azuread_service_principal.policy_api_test_client.object_id
  resource_object_id  = azuread_service_principal.policy_api.object_id
}
