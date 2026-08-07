resource "azuread_application" "ledger_api" {
  display_name = "ledger-api"
  owners       = [var.admin_object_id]

  api {
    requested_access_token_version = 2
  }

  app_role {
    allowed_member_types = ["Application"]
    description          = "Read account balances and the trial balance."
    display_name         = "Ledger Read"
    enabled              = true
    id                   = "41888ef5-c546-4777-be7a-643b91ac4634"
    value                = "Ledger.Read"
  }

  app_role {
    allowed_member_types = ["Application"]
    description          = "Post transactions to the ledger."
    display_name         = "Ledger Write"
    enabled              = true
    id                   = "6c7969fc-a5ce-4701-a37d-827dd5b28cbc"
    value                = "Ledger.Write"
  }
}

resource "azuread_service_principal" "ledger_api" {
  client_id = azuread_application.ledger_api.client_id
  owners    = [var.admin_object_id]
}

resource "azuread_application" "ledger_api_test_client" {
  display_name = "ledger-api-test-client"
  owners       = [var.admin_object_id]
}

resource "azuread_service_principal" "ledger_api_test_client" {
  client_id = azuread_application.ledger_api_test_client.client_id
  owners    = [var.admin_object_id]
}

resource "azuread_application_password" "ledger_api_test_client" {
  application_id = azuread_application.ledger_api_test_client.id
  display_name   = "ci-and-local-dev"
}

resource "azuread_app_role_assignment" "ledger_api_test_client_read" {
  app_role_id         = azuread_service_principal.ledger_api.app_role_ids["Ledger.Read"]
  principal_object_id = azuread_service_principal.ledger_api_test_client.object_id
  resource_object_id  = azuread_service_principal.ledger_api.object_id
}

resource "azuread_app_role_assignment" "ledger_api_test_client_write" {
  app_role_id         = azuread_service_principal.ledger_api.app_role_ids["Ledger.Write"]
  principal_object_id = azuread_service_principal.ledger_api_test_client.object_id
  resource_object_id  = azuread_service_principal.ledger_api.object_id
}
