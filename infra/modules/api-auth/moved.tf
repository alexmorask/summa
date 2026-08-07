moved {
  from = azuread_application.api
  to   = azuread_application.ledger_api
}

moved {
  from = azuread_service_principal.api
  to   = azuread_service_principal.ledger_api
}

moved {
  from = azuread_application.test_client
  to   = azuread_application.ledger_api_test_client
}

moved {
  from = azuread_service_principal.test_client
  to   = azuread_service_principal.ledger_api_test_client
}

moved {
  from = azuread_application_password.test_client
  to   = azuread_application_password.ledger_api_test_client
}

moved {
  from = azuread_app_role_assignment.test_client_read
  to   = azuread_app_role_assignment.ledger_api_test_client_read
}

moved {
  from = azuread_app_role_assignment.test_client_write
  to   = azuread_app_role_assignment.ledger_api_test_client_write
}
