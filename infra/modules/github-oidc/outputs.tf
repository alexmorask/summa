output "client_id" {
  description = "App Registration client ID, for the GitHub Environment's AZURE_CLIENT_ID variable."
  value       = azuread_application.github_actions.client_id
}
