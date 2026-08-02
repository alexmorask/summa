output "api_client_id" {
  description = "Client ID of the API's own App Registration — the JWT Bearer audience."
  value       = azuread_application.api.client_id
}

output "test_client_id" {
  description = "Client ID of the test-client App Registration, for CI and local dev token acquisition."
  value       = azuread_application.test_client.client_id
}

output "test_client_secret" {
  description = "Client secret of the test-client App Registration, for CI and local dev token acquisition."
  value       = azuread_application_password.test_client.value
  sensitive   = true
}
