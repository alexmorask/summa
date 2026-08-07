output "ledger_api_fqdn" {
  description = "Public FQDN of the deployed API's ingress."
  value       = azurerm_container_app.ledger_api.ingress[0].fqdn
}
