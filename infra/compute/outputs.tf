output "ledger_api_url" {
  description = "Public URL of the deployed API."
  value       = "https://${module.container_apps.ledger_api_fqdn}"
}
