output "policy_api_client_id" {
  description = "Client ID of the Policy API's own App Registration — the JWT Bearer audience."
  value       = azuread_application.policy_api.client_id
}

output "policy_api_test_client_id" {
  description = "Client ID of the test-client App Registration, for CI and local dev token acquisition."
  value       = azuread_application.policy_api_test_client.client_id
}

output "policy_api_test_client_secret" {
  description = "Client secret of the test-client App Registration, for CI and local dev token acquisition."
  value       = azuread_application_password.policy_api_test_client.value
  sensitive   = true
}
