output "api_fqdn" {
  description = "Public FQDN of the deployed API's ingress."
  value       = azurerm_container_app.api.ingress[0].fqdn
}
